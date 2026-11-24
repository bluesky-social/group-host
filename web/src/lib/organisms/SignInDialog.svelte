<script lang="ts">
  import Dialog from '#lib/molecules/Dialog.svelte';
  import HandleInput from '#lib/molecules/HandleInput.svelte';
  import { SIGN_IN } from '#lib/signIn.ts';
  import { m } from '#lib/paraglide/messages.js';
  /**
   * Signing in with an Atmosphere account, as oneself: the handle, sent to
   * the group host's `/console/login`, which goes to the account's own
   * server to approve it and back to `back`, the page it was opened on.
   * Picking an account the field suggests signs in as it at once. Someone
   * with no account makes one at `signUpAt`, the host's sign-up PDS, and
   * comes back signed in.
   */
  let { back = '/', signUpAt = 'https://bsky.social' }: { back?: string; signUpAt?: string } =
    $props();
  // A PDS's handles are, as a rule, under its own name.
  const example = $derived(`you.${new URL(signUpAt).hostname}`);
  const create = $derived(`/console/login?create&back=${encodeURIComponent(back)}`);
  // Apps on the Atmosphere, any of whose accounts signs in here: a row
  // running past, twice over so it wraps without a seam.
  const apps = [
    'bluesky',
    'tangled',
    'leaflet',
    'semble',
    'skylight',
    'popfeed',
    'anisota',
    'flashes',
    'spark',
    'stream-place',
    'blento',
    'germ',
    'gander',
    'offprint',
    'pckt',
    'plyr',
    'beacon-bits',
    'eurosky',
    'northsky',
    'blacksky'
  ];
  const icon = (app: string) => `/atmo/${app}.${app === 'spark' ? 'png' : 'jpg'}`;
</script>

<Dialog id={SIGN_IN} title={m.sign_in_dialog_title()}>
  <div class="apps" aria-hidden="true">
    <div class="apps-run">
      {#each [...apps, ...apps] as app, i (i)}<img
          src={icon(app)}
          alt=""
          width="40"
          height="40"
          loading="lazy"
        />{/each}
    </div>
  </div>
  <p class="what">
    <a href="https://atmosphereaccount.com/" target="_blank" rel="noopener noreferrer"
      >{m.sign_in_dialog_what()}</a
    >
  </p>
  <form class="stack" method="get" action="/console/login">
    <input type="hidden" name="back" value={back} />
    <HandleInput
      id="sign-in-handle"
      label={m.sign_in_dialog_handle()}
      autocomplete="username"
      placeholder={example}
      onpick={(f) => f.form?.requestSubmit()}
    />
    <div class="actions">
      <button class="quiet" type="button" popovertarget={SIGN_IN} popovertargetaction="hide"
        >{m.common_cancel()}</button
      >
      <button type="submit">{m.common_sign_in()}</button>
    </div>
  </form>
  <p class="create">
    {m.sign_in_dialog_no_account()}
    <a href={create}>{m.sign_in_dialog_create()}</a>
  </p>
</Dialog>
