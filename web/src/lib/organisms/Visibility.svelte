<script lang="ts">
  import { enhance } from '$app/forms';
  import SaveBar from '#lib/molecules/SaveBar.svelte';
  import { Saving } from '#lib/saving.svelte.ts';
  import { m } from '#lib/paraglide/messages.js';
  /**
   * Who sees a group's profile and rules: anyone, when it is also declared
   * on the network for others to find, or its members alone, when it shows
   * elsewhere by its handle alone. `listed` is which it is.
   */
  let { listed }: { listed: boolean } = $props();
  const saving = new Saving();
</script>

<form
  class="stack"
  method="post"
  action="?/visibility"
  use:enhance={saving.submit}
  use:saving.track
>
  <label
    ><input type="radio" name="seen" value="public" checked={listed} />
    {m.visibility_public()}</label
  >
  <label
    ><input type="radio" name="seen" value="members" checked={!listed} />
    {m.visibility_members()}</label
  >
  <p class="mute">{m.visibility_hint()}</p>
  <SaveBar {saving} what={m.visibility_what()} />
</form>
