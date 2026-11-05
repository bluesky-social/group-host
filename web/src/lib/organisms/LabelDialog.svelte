<script lang="ts">
  import FormDialog from '#lib/molecules/FormDialog.svelte';
  import HandleInput from '#lib/molecules/HandleInput.svelte';
  /** Labelling someone, by their handle, as the group, with a value: opened by a button whose `popovertarget` is `id`. */
  let { id }: { id: string } = $props();
</script>

<FormDialog {id} title="Label someone" action="?/label" submit="Label them">
  {#snippet intro()}The group puts the label on their account. Apps that follow the group's labels
    can act on it.{/snippet}
  <HandleInput id="{id}-handle" label="Their handle" placeholder="someone.bsky.social" />
  <label class="field">Label <input name="val" required maxlength="128" placeholder="spam" /></label
  >
</FormDialog>
