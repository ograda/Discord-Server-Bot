import { describe, it, expect, vi } from 'vitest';
import { createInviteController, inviteError, type InviteSdk, type InviteState } from './invite';

function fixture(overrides: Partial<InviteSdk> = {}) {
  const sdk = { guildId: '1', channelId: '2', ready: vi.fn(async () => {}), commands: {
    openInviteDialog: vi.fn(async () => {}),
  }, ...overrides };
  const states: InviteState[] = [];
  const controller = createInviteController(sdk, state => states.push(state), 50);
  return { sdk, states, controller };
}
describe('Activity de convites', () => {
  it('aguarda ready e chama o SDK somente após interação explícita', async () => {
    const f = fixture();
    await f.controller.open();
    expect(f.sdk.commands.openInviteDialog).not.toHaveBeenCalled();
    await f.controller.start();
    expect(f.sdk.ready).toHaveBeenCalledOnce();
    expect(f.sdk.commands.openInviteDialog).not.toHaveBeenCalled();
    await f.controller.open();
    expect(f.sdk.commands.openInviteDialog).toHaveBeenCalledWith();
    expect(f.states.at(-1)?.phase).toBe('ready');
  });
  it.each([{ guildId: null }, { channelId: null }])('bloqueia DMs ou contexto incompleto: %j', async missing => {
    const f = fixture(missing);
    await f.controller.start(); await f.controller.open();
    expect(f.states.at(-1)?.phase).toBe('unavailable');
    expect(f.sdk.commands.openInviteDialog).not.toHaveBeenCalled();
  });
  it('trata falha/timeout do SDK e permite nova tentativa após falha do diálogo', async () => {
    const f = fixture({ commands: { openInviteDialog: vi.fn().mockRejectedValue({ code: 4006, secret: 'never display' }) } });
    await f.controller.start(); await f.controller.open();
    expect(f.states.at(-1)?.message).toContain('não autorizou');
    expect(f.states.at(-1)?.message).not.toContain('secret');
    expect(f.states.at(-1)?.phase).toBe('ready');
    const timedOut = fixture({ ready: () => new Promise(() => {}) });
    await timedOut.controller.start();
    expect(timedOut.states.at(-1)?.phase).toBe('unavailable');
  });
  it('impede cliques concorrentes e informa indisponibilidade sem dados internos', async () => {
    let finish!: () => void;
    const openInviteDialog = vi.fn(() => new Promise<void>(resolve => { finish = resolve; }));
    const f = fixture({ commands: { openInviteDialog } });
    await f.controller.start();
    const first = f.controller.open();
    await f.controller.open();
    expect(openInviteDialog).toHaveBeenCalledOnce();
    finish(); await first;
    expect(inviteError(new Error('secret'))).not.toContain('secret');
  });
});
