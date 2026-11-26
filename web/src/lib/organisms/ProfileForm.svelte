<script lang="ts">
  import { enhance } from '$app/forms';
  import SaveBar from '#lib/molecules/SaveBar.svelte';
  import { Saving } from '#lib/saving.svelte.ts';
  import { m } from '#lib/paraglide/messages.js';
  /**
   * A group's profile, to change: its name, its words, how people join,
   * and its own site.
   */
  let {
    displayName,
    description,
    joinPolicy,
    website
  }: {
    displayName?: string;
    description?: string;
    joinPolicy?: string;
    website?: string;
  } = $props();
  const saving = new Saving();
  const policy = $derived(joinPolicy ?? 'approval');
</script>

<form class="stack" method="post" action="?/profile" use:enhance={saving.submit} use:saving.track>
  <label class="field"
    >{m.profile_form_name()}
    <input name="displayName" required maxlength="64" value={displayName ?? ''} /></label
  >
  <label class="field"
    ><span>{m.profile_form_description()} <span class="optional">{m.common_optional()}</span></span>
    <textarea name="description" maxlength="2000">{description ?? ''}</textarea></label
  >
  <label class="field">
    {m.profile_form_join()}
    <select name="joinPolicy" defaultValue={policy}>
      <option value="approval">{m.profile_form_join_approval()}</option>
      <option value="open">{m.profile_form_join_open()}</option>
      <option value="invite">{m.profile_form_join_invite()}</option>
    </select>
  </label>
  <label class="field"
    ><span>{m.profile_form_website()} <span class="optional">{m.common_optional()}</span></span>
    <input name="website" type="url" value={website ?? ''} /></label
  >
  <SaveBar {saving} what={m.profile_form_saves()} />
</form>
