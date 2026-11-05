<script lang="ts">
  import { enhance } from '$app/forms';
  import { inPlace } from '#lib/forms.ts';
  /**
   * Who sees a group's profile and rules: anyone, when it is also declared
   * on the network for others to find, or its members alone, when it shows
   * elsewhere by its handle alone. `listed` is which it is.
   */
  let { listed }: { listed: boolean } = $props();
</script>

<form class="stack" method="post" action="?/visibility" use:enhance={inPlace}>
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
  <button class="quiet" type="submit">Save</button>
</form>
