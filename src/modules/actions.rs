macro_rules! actions {
    ($name:ident { $($variant:ident => $value:literal),+ $(,)? }) => {
        #[derive(Debug, Clone, Copy, poise::ChoiceParameter)]
        pub enum $name { $(#[name = $value] $variant),+ }
        impl $name { pub fn as_str(self) -> &'static str { match self { $(Self::$variant => $value),+ } } }
    };
}
actions!(RulesAction { Preview => "preview", Publish => "publish", Update => "update", Reset => "reset", Remove => "remove" });
actions!(LinksAction { Preview => "preview", Publish => "publish", Update => "update", Reset => "reset", Add => "add", Remove => "remove" });
actions!(AnnouncementAction { Create => "create", Edit => "edit", Remove => "remove" });
actions!(ProfileAction { Revert => "revert", Nickname => "nickname", Bio => "bio", Avatar => "avatar" });
