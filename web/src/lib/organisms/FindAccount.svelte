<script lang="ts">
  import { enhance } from '$app/forms';
  import HandleInput from '#lib/molecules/HandleInput.svelte';
  import { inPlace } from '#lib/forms.ts';
  /** Finding someone in the group by their handle, to review them whether or not anyone reported them: picked, they are reviewed at once. */
  let form = $state<HTMLFormElement>();
</script>

<form
  class="find row-form"
  method="post"
  action="?/find"
  role="search"
  bind:this={form}
  use:enhance={inPlace}
>
  <HandleInput
    id="find-handle"
    label="Find an account"
    placeholder="Their handle"
    onpick={() => form?.requestSubmit()}
  />
  <button class="quiet" type="submit">Review</button>
</form>
