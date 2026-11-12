export interface InviteSdk {
  guildId: string | null;
  channelId: string | null;
  ready(): Promise<void>;
  commands: { openInviteDialog(): Promise<unknown> };
}

export interface InviteState {
  phase: 'loading' | 'ready' | 'opening' | 'unavailable';
  message: string;
}

const within = async <T>(work: Promise<T>, milliseconds: number): Promise<T> => {
  let timer: ReturnType<typeof setTimeout> | undefined;
  try {
    return await Promise.race([
      work,
      new Promise<never>((_, reject) => {
        timer = setTimeout(() => reject(new Error('timeout')), milliseconds);
      }),
    ]);
  } finally {
    clearTimeout(timer);
  }
};

// No raw SDK exception is displayed/logged: it can contain client context.
export function inviteError(error: unknown): string {
  const code = typeof error === 'object' && error !== null && 'code' in error
    ? Number(error.code) : undefined;
  if (code === 4006 || code === 50013 || code === 50001) {
    return 'O Discord não autorizou o convite. Confira sua permissão de Criar convite neste canal e as permissões da Activity.';
  }
  return 'O diálogo de convites não está disponível neste cliente ou contexto. Confira as permissões e tente novamente dentro do Discord.';
}

export function createInviteController(
  sdk: InviteSdk,
  render: (state: InviteState) => void,
  timeout = 10_000,
) {
  let phase: InviteState['phase'] = 'loading';
  const update = (next: InviteState['phase'], message: string) => {
    phase = next;
    render({ phase, message });
  };
  return {
    async start() {
      update('loading', 'Conectando ao Discord…');
      try {
        await within(sdk.ready(), timeout);
        if (!sdk.guildId || !sdk.channelId) {
          update('unavailable', 'Abra esta Activity em um canal de servidor. Convites não estão disponíveis em mensagens diretas.');
          return;
        }
        update('ready', 'Pronto. Abra a janela nativa de convites pelo botão acima.');
      } catch {
        update('unavailable', 'Não foi possível conectar ao Discord. Abra a Activity pelo botão Convidar amigos em um servidor elegível.');
      }
    },
    async open() {
      if (phase !== 'ready') return;
      update('opening', 'Abrindo a janela de convites…');
      try {
        // User gesture is intentional; never simulate friends or bypass permissions.
        await within(sdk.commands.openInviteDialog(), timeout);
        update('ready', 'Janela de convites aberta no Discord.');
      } catch (error) {
        update('ready', inviteError(error));
      }
    },
  };
}
