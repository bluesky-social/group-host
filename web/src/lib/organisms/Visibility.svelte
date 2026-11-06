<script lang="ts">
  import { enhance } from '$app/forms';
  import SaveBar from '#lib/molecules/SaveBar.svelte';
  import { Saving } from '#lib/saving.svelte.ts';
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
    ><input type="radio" name="seen" value="public" checked={listed} /> Anyone, and it can be found</label
  >
  <label
    ><input type="radio" name="seen" value="members" checked={!listed} /> Members only. Others see just
    its handle.</label
  >
  <p class="mute">
    Hiding it stops new people finding it. It can't undo what people have already seen.
  </p>
  <SaveBar {saving} what="who sees it" />
</form>
