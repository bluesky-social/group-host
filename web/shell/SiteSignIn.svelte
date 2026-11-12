<script lang="ts">
  import SignInDialog from '#lib/organisms/SignInDialog.svelte';
  import { signing } from './signing.svelte.ts';
  /**
   * The console's own sign-in dialog, over a group's site: the handle, or
   * asking to make an account, go to the page around the site, which signs
   * the visitor in at their own server.
   */
  let {
    signUpAt,
    assets,
    onhandle,
    oncreate
  }: {
    signUpAt: string;
    assets: string;
    onhandle: (handle: string) => void;
    oncreate: () => void;
  } = $props();
</script>

<SignInDialog {signUpAt} {assets} {onhandle} {oncreate} failed={signing.failed} />
