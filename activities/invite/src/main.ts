import { DiscordSDK } from '@discord/embedded-app-sdk';
import { createInviteController, type InviteState, type InviteSdk } from './invite';
import './style.css';

const button = document.querySelector<HTMLButtonElement>('#invite')!;
const status = document.querySelector<HTMLParagraphElement>('#status')!;
const render = (state: InviteState) => {
  button.disabled = state.phase !== 'ready';
  status.textContent = state.message;
};

async function main() {
  try {
    let sdk: InviteSdk;
    // Development-only UI exercise. Production never substitutes the real SDK.
    if (import.meta.env.DEV && new URLSearchParams(location.search).has('demo')) {
      sdk = { guildId: 'demo', channelId: 'demo', ready: async () => {}, commands: {
        openInviteDialog: async () => { throw new Error('Demo sem conexão com o Discord'); },
      } };
      status.textContent = 'Demonstração local; não abre convites reais.';
    } else {
      const clientId = import.meta.env.VITE_DISCORD_CLIENT_ID;
      if (!/^\d{17,20}$/.test(clientId ?? '') || /^0+$/.test(clientId)) {
        throw new Error('Application ID público não configurado');
      }
      sdk = new DiscordSDK(clientId);
    }
    const controller = createInviteController(sdk, render);
    button.addEventListener('click', () => { void controller.open(); });
    await controller.start();
  } catch {
    render({ phase: 'unavailable', message: 'Activity indisponível. Confira o Application ID público, o URL Mapping e abra a aplicação dentro do Discord.' });
  }
}
void main();
