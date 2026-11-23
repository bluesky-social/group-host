<script lang="ts">
  import { m } from '#lib/paraglide/messages.js';
  import HandleInput from '#lib/molecules/HandleInput.svelte';
  /**
   * Signing in to an account to move here, at its own server: its handle,
   * sent to the group host's `/console/move/login`, which goes there and
   * back to `back`.
   */
  let { back = '/move' }: { back?: string } = $props();
</script>

<form class="stack" method="get" action="/console/move/login">
  <input type="hidden" name="back" value={back} />
  <HandleInput id="move-handle" label={m.move_sign_in_label()} placeholder="riders.bsky.social" />
  <p>{m.move_sign_in_body()}</p>
  <button type="submit">{m.move_sign_in_submit()}</button>
</form>
