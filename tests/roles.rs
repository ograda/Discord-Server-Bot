use anyhow::Result;
use discord_server_bot::{
    config::{self, Roles},
    core::{commands, permissions::Authority},
    services::{
        roles::{self, Action, Api, RoleLocks},
        storage::{State, Store},
    },
};
use poise::serenity_prelude as s;
use std::{
    collections::{BTreeMap, HashMap},
    sync::{
        Mutex,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
};

fn member(id: u64) -> s::Member {
    let mut member = s::Member::default();
    member.guild_id = s::GuildId::new(1);
    member.user.id = s::UserId::new(id);
    member
}
fn authority() -> Authority {
    let mut guild: s::PartialGuild = serde_json::from_value(serde_json::json!({
        "id":"1","name":"Test","owner_id":"99","verification_level":0,
        "default_message_notifications":0,"explicit_content_filter":0,"roles":[],"emojis":[],
        "features":[],"mfa_level":0,"system_channel_flags":0,"premium_tier":0,"preferred_locale":"pt-BR",
        "nsfw_level":0,"stickers":[],"premium_progress_bar_enabled":false
    })).unwrap();
    for (id, position, permissions) in [
        (1, 0, s::Permissions::empty()),
        (10, 1, s::Permissions::empty()),
        (20, 2, s::Permissions::empty()),
        (
            30,
            5,
            s::Permissions::MANAGE_ROLES | s::Permissions::MANAGE_GUILD,
        ),
        (40, 10, s::Permissions::MANAGE_ROLES),
    ] {
        let mut role = s::Role::default();
        role.guild_id = guild.id;
        role.id = s::RoleId::new(id);
        role.position = position;
        role.permissions = permissions;
        guild.roles.insert(role.id, role);
    }
    let mut actor = member(8);
    actor.roles.push(s::RoleId::new(30));
    let mut bot = member(9);
    bot.roles.push(s::RoleId::new(40));
    bot.user.bot = true;
    Authority { guild, actor, bot }
}
async fn http_error(status: u16) -> anyhow::Error {
    let response: reqwest::Response = http::Response::builder()
        .status(status)
        .header("content-type", "application/json")
        .body(r#"{"code":50013,"message":"SECRET_TOKEN"}"#.to_owned())
        .unwrap()
        .into();
    s::Error::Http(s::HttpError::UnsuccessfulRequest(
        s::ErrorResponse::from_response(response, reqwest::Method::PUT).await,
    ))
    .into()
}
struct Fake {
    auth: Mutex<Authority>,
    listed: Vec<s::Member>,
    current: Mutex<BTreeMap<s::UserId, s::Member>>,
    cursors: Mutex<Vec<Option<s::UserId>>>,
    writes: Mutex<Vec<(s::UserId, s::RoleId, Action)>>,
    write_errors: HashMap<s::UserId, u16>,
    member_errors: HashMap<s::UserId, u16>,
    fail_second_page: bool,
    repeat_page: bool,
    revoke_after_write: AtomicBool,
    in_flight: AtomicUsize,
    maximum_in_flight: AtomicUsize,
}
impl Fake {
    fn new(members: Vec<s::Member>) -> Self {
        Self {
            auth: Mutex::new(authority()),
            current: Mutex::new(members.iter().map(|m| (m.user.id, m.clone())).collect()),
            listed: members,
            cursors: Mutex::new(vec![]),
            writes: Mutex::new(vec![]),
            write_errors: HashMap::new(),
            member_errors: HashMap::new(),
            fail_second_page: false,
            repeat_page: false,
            revoke_after_write: AtomicBool::new(false),
            in_flight: AtomicUsize::new(0),
            maximum_in_flight: AtomicUsize::new(0),
        }
    }
}
impl Api for Fake {
    async fn authority(&self) -> Result<Authority> {
        Ok(self.auth.lock().unwrap().clone())
    }
    async fn members(&self, after: Option<s::UserId>) -> Result<Vec<s::Member>> {
        self.cursors.lock().unwrap().push(after);
        if after.is_some() && self.fail_second_page {
            return Err(http_error(403).await);
        }
        let after = if self.repeat_page { None } else { after };
        Ok(self
            .listed
            .iter()
            .filter(|m| after.is_none_or(|a| m.user.id > a))
            .take(1000)
            .cloned()
            .collect())
    }
    async fn member(&self, user: s::UserId) -> Result<s::Member> {
        if let Some(status) = self.member_errors.get(&user) {
            return Err(http_error(*status).await);
        }
        let member = self.current.lock().unwrap().get(&user).cloned();
        match member {
            Some(member) => Ok(member),
            None => Err(http_error(404).await),
        }
    }
    async fn change(
        &self,
        user: s::UserId,
        role: s::RoleId,
        action: Action,
        reason: &str,
    ) -> Result<()> {
        assert!(reason.contains("actor=8"));
        let active = self.in_flight.fetch_add(1, Ordering::SeqCst) + 1;
        self.maximum_in_flight.fetch_max(active, Ordering::SeqCst);
        tokio::task::yield_now().await;
        self.in_flight.fetch_sub(1, Ordering::SeqCst);
        self.writes.lock().unwrap().push((user, role, action));
        if let Some(status) = self.write_errors.get(&user) {
            return Err(http_error(*status).await);
        }
        {
            let mut members = self.current.lock().unwrap();
            let member = members.get_mut(&user).unwrap();
            if action.wants_role() {
                member.roles.push(role);
            } else {
                member.roles.retain(|r| *r != role);
            }
        }
        if self.revoke_after_write.swap(false, Ordering::SeqCst) {
            self.auth
                .lock()
                .unwrap()
                .guild
                .roles
                .get_mut(&s::RoleId::new(30))
                .unwrap()
                .permissions = s::Permissions::empty();
        }
        Ok(())
    }
}

#[test]
fn slash_roles_use_required_native_user_and_role_without_subcommands_or_strings() {
    let all = commands::all();
    for name in ["role", "role-all"] {
        let command = all.iter().find(|c| c.name == name).unwrap();
        assert!(command.subcommands.is_empty());
        let json = serde_json::to_value(command.create_as_slash_command().unwrap()).unwrap();
        let options = json["options"].as_array().unwrap();
        assert_eq!(options.len(), if name == "role" { 3 } else { 2 });
        assert!(
            options
                .iter()
                .all(|o| o["required"] == true && o["type"] != 3)
        );
        assert_eq!(
            options.iter().find(|o| o["name"] == "cargo").unwrap()["type"],
            8
        );
        let action = options.iter().find(|o| o["name"] == "ação").unwrap();
        assert_eq!(action["choices"][0]["name"], "add");
        assert_eq!(action["choices"][1]["name"], "remove");
        if name == "role" {
            assert_eq!(
                options.iter().find(|o| o["name"] == "usuário").unwrap()["type"],
                6
            );
        } else {
            assert!(!options.iter().any(|o| o["name"] == "usuário"));
        }
        assert_eq!(config::availability::default_action(name), None);
    }
}
#[test]
fn role_commands_and_actions_are_independent_of_each_other_and_intent() {
    let global: config::GlobalConfig =
        toml::from_str(include_str!("fixtures/global.toml")).unwrap();
    assert!(
        !global
            .gateway_intents()
            .contains(s::GatewayIntents::GUILD_MEMBERS)
    );
    let enabled: config::GlobalConfig = toml::from_str(&format!(
        "{}\n[intents]\nguild_members=true",
        include_str!("fixtures/global.toml")
    ))
    .unwrap();
    assert!(
        enabled
            .gateway_intents()
            .contains(s::GatewayIntents::GUILD_MEMBERS)
    );
    let mut guild: config::GuildConfig = toml::from_str("guild_id='1'").unwrap();
    assert!(guild.command_enabled("role.add", global.modules));
    assert!(!guild.command_enabled("role-all.add", global.modules));
    config::availability::apply_commands(
        &mut guild,
        toml::from_str("[commands]\nrole=false\nrole-all=true\n[actions.role-all]\nadd=false")
            .unwrap(),
    )
    .unwrap();
    assert!(!guild.command_enabled("role.add", global.modules));
    assert!(!guild.command_enabled("role-all.add", global.modules));
    assert!(guild.command_enabled("role-all.remove", global.modules));
    let remote = serde_json::to_value(config::availability::remote_commands(
        &guild,
        global.modules,
    ))
    .unwrap();
    assert!(
        !remote
            .as_array()
            .unwrap()
            .iter()
            .any(|c| c["name"] == "role")
    );
    let all = remote
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["name"] == "role-all")
        .unwrap();
    assert_eq!(all["options"][0]["choices"].as_array().unwrap().len(), 1);
    assert_eq!(all["options"][0]["choices"][0]["value"], 1); // Do not renumber Remove to Add.
    guild.modules = Some(config::Modules {
        roles: false,
        ..global.modules
    });
    assert!(!guild.command_enabled("role-all.remove", global.modules));
}
#[test]
fn protected_managed_special_roles_and_admin_bulk_roles_are_blocked() {
    for target in [1, 30, 40] {
        assert!(
            roles::validate_role(
                &authority(),
                &Roles::default(),
                s::RoleId::new(target),
                true
            )
            .is_err()
        );
    }
    let mut auth = authority();
    let id = s::RoleId::new(10);
    assert!(roles::validate_role(&auth, &Roles::default(), id, true).is_ok());
    let protected: Roles = toml::from_str("protected=['10']").unwrap();
    assert!(roles::validate_role(&auth, &protected, id, false).is_err());
    let moderators: Roles = toml::from_str("moderators=['10','30']").unwrap();
    assert!(roles::validate_role(&auth, &moderators, id, true).is_err());
    for mode in 0..7 {
        let mut role = auth.guild.roles[&id].clone();
        match mode {
            0 => role.managed = true,
            1 => role.tags.bot_id = Some(s::UserId::new(9)),
            2 => role.tags.integration_id = Some(s::IntegrationId::new(1)),
            3 => role.tags.premium_subscriber = true,
            4 => role.tags.available_for_purchase = true,
            5 => role.tags.guild_connections = true,
            _ => role.permissions = s::Permissions::ADMINISTRATOR,
        }
        auth.guild.roles.insert(id, role);
        assert!(roles::validate_role(&auth, &Roles::default(), id, true).is_err());
        auth = authority();
    }
    auth.guild.roles.get_mut(&id).unwrap().guild_id = s::GuildId::new(2);
    assert!(roles::validate_role(&auth, &Roles::default(), id, true).is_err());
}
#[test]
fn bulk_authorization_is_stronger_and_preserves_moderator_and_bot_checks() {
    let mut auth = authority();
    auth.guild
        .roles
        .get_mut(&s::RoleId::new(30))
        .unwrap()
        .permissions = s::Permissions::MANAGE_ROLES;
    assert!(roles::validate_actor(&auth, &Roles::default(), false).is_ok());
    assert!(roles::validate_actor(&auth, &Roles::default(), true).is_err());
    auth = authority();
    let policy: Roles = toml::from_str("moderators=['10']").unwrap();
    assert!(roles::validate_actor(&auth, &policy, true).is_err());
    auth.actor.user.id = auth.guild.owner_id;
    assert!(roles::validate_actor(&auth, &policy, true).is_ok());
    auth.guild
        .roles
        .get_mut(&s::RoleId::new(40))
        .unwrap()
        .permissions = s::Permissions::empty();
    assert!(roles::validate_actor(&auth, &policy, true).is_err());
}
#[tokio::test]
async fn pagination_reads_all_members_and_planning_does_not_modify_roles() {
    let mut members: Vec<_> = (100..1103).map(member).collect();
    members[0].user.bot = true;
    members[1].roles.push(s::RoleId::new(10));
    let api = Fake::new(members);
    let plan = roles::plan(&api, &Roles::default(), s::RoleId::new(10), Action::Add)
        .await
        .unwrap();
    assert_eq!(plan.scanned, 1003);
    assert_eq!(plan.targets.len(), 1001);
    assert_eq!(plan.skipped_count(), 2);
    assert_eq!(
        *api.cursors.lock().unwrap(),
        vec![None, Some(s::UserId::new(1099))]
    );
    assert!(api.writes.lock().unwrap().is_empty()); // No implicit execution after planning.
}
#[tokio::test]
async fn partial_or_nonadvancing_pagination_cannot_produce_an_executable_plan() {
    for repeat in [false, true] {
        let mut api = Fake::new((100..1101).map(member).collect());
        api.fail_second_page = !repeat;
        api.repeat_page = repeat;
        let error = roles::plan(&api, &Roles::default(), s::RoleId::new(10), Action::Add)
            .await
            .err()
            .unwrap();
        assert!(!error.to_string().contains("SECRET"));
        assert!(api.writes.lock().unwrap().is_empty());
    }
}
#[test]
fn human_eligibility_excludes_owners_actor_bots_screening_and_hierarchy() {
    let auth = authority();
    let id = s::RoleId::new(10);
    for (mut member, reason) in [
        (member(99), "protected_member"),
        (member(8), "protected_member"),
        (member(9), "protected_member"),
        (member(100), "bot"),
        (member(101), "membership_screening"),
        (member(102), "bot_hierarchy"),
        (member(103), "actor_hierarchy"),
        (member(104), "unknown_member_role"),
    ] {
        match reason {
            "bot" => member.user.bot = true,
            "membership_screening" => member.pending = true,
            "bot_hierarchy" => member.roles.push(s::RoleId::new(40)),
            "actor_hierarchy" => member.roles.push(s::RoleId::new(30)),
            "unknown_member_role" => member.roles.push(s::RoleId::new(999)),
            _ => (),
        }
        assert_eq!(
            roles::skip_reason(&auth, &member, id, Action::Add, true),
            Some(reason)
        );
    }
}
#[tokio::test]
async fn individual_role_changes_are_native_targets_and_do_not_list_all_members() {
    let api = Fake::new(vec![member(100)]);
    let guild = s::GuildId::new(1);
    let id = s::RoleId::new(10);
    let user = s::UserId::new(100);
    assert!(
        roles::change_one(&api, &Roles::default(), guild, id, user, Action::Add)
            .await
            .unwrap()
    );
    assert!(
        !roles::change_one(&api, &Roles::default(), guild, id, user, Action::Add)
            .await
            .unwrap()
    );
    assert!(
        roles::change_one(&api, &Roles::default(), guild, id, user, Action::Remove)
            .await
            .unwrap()
    );
    assert_eq!(api.writes.lock().unwrap().len(), 2);
    assert!(api.cursors.lock().unwrap().is_empty());
    assert!(
        roles::change_one(
            &api,
            &Roles::default(),
            guild,
            id,
            s::UserId::new(999),
            Action::Add
        )
        .await
        .is_err()
    );
    api.current.lock().unwrap().get_mut(&user).unwrap().guild_id = s::GuildId::new(2);
    assert!(
        roles::change_one(&api, &Roles::default(), guild, id, user, Action::Add)
            .await
            .is_err()
    );
    assert_eq!(api.writes.lock().unwrap().len(), 2);
}
#[tokio::test]
async fn execution_revalidates_state_counts_failures_and_preserves_guild_isolation() {
    let mut api = Fake::new((100..105).map(member).collect());
    api.write_errors.insert(s::UserId::new(101), 400);
    api.member_errors.insert(s::UserId::new(103), 404);
    let plan = roles::plan(&api, &Roles::default(), s::RoleId::new(10), Action::Add)
        .await
        .unwrap();
    api.current
        .lock()
        .unwrap()
        .get_mut(&s::UserId::new(102))
        .unwrap()
        .roles
        .push(s::RoleId::new(10));
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path().into()).unwrap();
    let mut state = State::default();
    state.profile.nickname = Some("Other guild".into());
    store.write(s::GuildId::new(2), &state).unwrap();
    let other = std::fs::read(dir.path().join("guilds/2/state.json")).unwrap();
    let result = roles::execute(
        &api,
        &Roles::default(),
        plan,
        "test-operation".into(),
        &store,
    )
    .await
    .unwrap();
    assert!(result.saved);
    let report = result.report;
    assert_eq!(report.success, 2);
    assert_eq!(report.failed, 1);
    assert_eq!(report.skipped.values().sum::<u64>(), 2);
    assert_eq!(report.unprocessed, 0);
    assert_eq!(report.status, "completed");
    assert_eq!(api.maximum_in_flight.load(Ordering::SeqCst), 1);
    assert_eq!(
        store.read(s::GuildId::new(1)).unwrap().role_operations["test-operation"].success,
        2
    );
    assert_eq!(
        std::fs::read(dir.path().join("guilds/2/state.json")).unwrap(),
        other
    );
    assert!(report.summary().contains("Sucessos: 2"));
}
#[tokio::test]
async fn permission_revocation_or_http_403_stops_further_bulk_writes() {
    for forbidden in [false, true] {
        let mut api = Fake::new(vec![member(100), member(101), member(102)]);
        if forbidden {
            api.write_errors.insert(s::UserId::new(100), 403);
        } else {
            api.revoke_after_write.store(true, Ordering::SeqCst);
        }
        let plan = roles::plan(&api, &Roles::default(), s::RoleId::new(10), Action::Add)
            .await
            .unwrap();
        let dir = tempfile::tempdir().unwrap();
        let store = Store::open(dir.path().into()).unwrap();
        let result = roles::execute(&api, &Roles::default(), plan, "stopped".into(), &store)
            .await
            .unwrap();
        assert_eq!(result.report.status, "stopped");
        assert_eq!(result.report.unprocessed, 2);
        assert_eq!(api.writes.lock().unwrap().len(), 1);
        assert!(!result.report.summary().contains("SECRET"));
    }
}
#[tokio::test]
async fn remove_is_idempotent_and_does_not_touch_bots_or_new_members_after_confirmation() {
    let mut human = member(100);
    human.roles.push(s::RoleId::new(10));
    let mut bot = member(101);
    bot.roles.push(s::RoleId::new(10));
    bot.user.bot = true;
    let api = Fake::new(vec![human, bot, member(102)]);
    let plan = roles::plan(&api, &Roles::default(), s::RoleId::new(10), Action::Remove)
        .await
        .unwrap();
    assert_eq!(plan.targets, vec![s::UserId::new(100)]);
    let mut joined = member(103);
    joined.roles.push(s::RoleId::new(10));
    api.current.lock().unwrap().insert(joined.user.id, joined);
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path().into()).unwrap();
    let report = roles::execute(&api, &Roles::default(), plan, "remove".into(), &store)
        .await
        .unwrap()
        .report;
    assert_eq!(report.success, 1);
    assert_eq!(report.skipped.values().sum::<u64>(), 2);
    assert_eq!(api.writes.lock().unwrap().len(), 1);
    assert!(
        api.current.lock().unwrap()[&s::UserId::new(103)]
            .roles
            .contains(&s::RoleId::new(10))
    );
}
#[test]
fn guild_lock_covers_bulk_and_individual_operations_and_releases_on_cancel_or_error() {
    let locks = RoleLocks::default();
    let one = s::GuildId::new(1);
    let two = s::GuildId::new(2);
    let pending_confirmation = locks.try_acquire(one).unwrap();
    assert!(locks.try_acquire(one).is_err());
    let other = locks.try_acquire(two).unwrap();
    drop(other);
    drop(pending_confirmation);
    let operation = locks.try_acquire(one).unwrap();
    assert!(locks.try_acquire(one).is_err());
    drop(operation);
    assert!(locks.try_acquire(one).is_ok());
}

#[tokio::test]
async fn discord_adapter_uses_paginated_rest_and_single_role_put_delete_endpoints() {
    use std::io::{Read, Write};
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let body = serde_json::to_string(&vec![member(1001)]).unwrap();
    let server = std::thread::spawn(move || {
        let mut requests = vec![];
        for (status, response_body) in [
            (200, body.clone()),
            (200, body),
            (204, String::new()),
            (204, String::new()),
        ] {
            let (mut socket, _) = listener.accept().unwrap();
            socket
                .set_read_timeout(Some(std::time::Duration::from_secs(5)))
                .unwrap();
            let mut bytes = vec![];
            let header = loop {
                let mut buffer = [0; 4096];
                let n = socket.read(&mut buffer).unwrap();
                assert!(n > 0);
                bytes.extend_from_slice(&buffer[..n]);
                if let Some(index) = bytes.windows(4).position(|b| b == b"\r\n\r\n") {
                    break String::from_utf8(bytes[..index].to_vec()).unwrap();
                }
            };
            requests.push(header);
            write!(socket,"HTTP/1.1 {status} OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{response_body}", response_body.len()).unwrap();
        }
        requests
    });
    // This API proxy + disabled limiter is restricted to localhost in this test.
    // Production always uses ctx.http(), whose standard Serenity limiter remains enabled.
    let http = s::HttpBuilder::new("TEST_TOKEN")
        .proxy(format!("http://{address}"))
        .ratelimiter_disabled(true)
        .build();
    let api = roles::DiscordApi {
        http: &http,
        guild: s::GuildId::new(1),
        actor: s::UserId::new(8),
        bot: s::UserId::new(9),
    };
    assert_eq!(
        api.members(None).await.unwrap()[0].guild_id,
        s::GuildId::new(1)
    );
    api.members(Some(s::UserId::new(1000))).await.unwrap();
    api.change(
        s::UserId::new(1001),
        s::RoleId::new(10),
        Action::Add,
        "role-all test",
    )
    .await
    .unwrap();
    api.change(
        s::UserId::new(1001),
        s::RoleId::new(10),
        Action::Remove,
        "role-all test",
    )
    .await
    .unwrap();
    let requests = server.join().unwrap();
    for (index, request) in requests.iter().take(2).enumerate() {
        assert!(request.starts_with("GET "));
        let target = request
            .lines()
            .next()
            .unwrap()
            .split_whitespace()
            .nth(1)
            .unwrap();
        let url = url::Url::parse(&format!("http://localhost{target}")).unwrap();
        assert_eq!(url.path(), "/api/v10/guilds/1/members");
        let params: HashMap<_, _> = url.query_pairs().collect();
        assert_eq!(params.get("limit").map(|s| s.as_ref()), Some("1000"));
        assert_eq!(
            params.get("after").map(|s| s.as_ref()),
            if index == 0 { None } else { Some("1000") }
        );
    }
    assert!(requests[2].starts_with("PUT /api/v10/guilds/1/members/1001/roles/10 "));
    assert!(requests[3].starts_with("DELETE /api/v10/guilds/1/members/1001/roles/10 "));
    assert!(requests[2].to_lowercase().contains("x-audit-log-reason:"));
}

#[tokio::test]
async fn unrecovered_rate_limit_stops_without_a_custom_retry_loop() {
    let mut api = Fake::new(vec![member(100), member(101)]);
    api.write_errors.insert(s::UserId::new(100), 429);
    let plan = roles::plan(&api, &Roles::default(), s::RoleId::new(10), Action::Add)
        .await
        .unwrap();
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path().into()).unwrap();
    let report = roles::execute(&api, &Roles::default(), plan, "rate-limit".into(), &store)
        .await
        .unwrap()
        .report;
    assert_eq!(report.status, "stopped");
    assert_eq!(report.failed, 1);
    assert_eq!(report.unprocessed, 1);
    assert!(report.summary().contains("HTTP 429"));
    assert_eq!(api.writes.lock().unwrap().len(), 1);
}
