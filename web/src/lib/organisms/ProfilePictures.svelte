<script lang="ts">
  import { enhance } from '$app/forms';
  import { inPlace } from '#lib/forms.ts';
  import Avatar from '#lib/atoms/Avatar.svelte';
  import FormDialog from '#lib/molecules/FormDialog.svelte';
  import { m } from '#lib/paraglide/messages.js';
  /**
   * A group's picture, to change: its avatar, round beside its name, as it
   * is now; a new one uploaded from a dialog, or the one there removed,
   * each button named by its verb alone beside the row's name, and in full
   * for a screen reader. `avatar` is where it is served, if set; `name`,
   * the group's.
   */
  let { name, avatar }: { name: string; avatar: string | null } = $props();
  const act = $derived(
    avatar ? m.profile_pictures_change_avatar() : m.profile_pictures_add_avatar()
  );
</script>

<ul class="pictures">
  <li>
    <Avatar src={avatar} size="md" {name} />
    <span class="row-main"
      ><strong>{m.profile_pictures_avatar()}</strong>{#if !avatar}<span class="what"
          >{m.profile_pictures_none()}</span
        >{/if}</span
    >
    <span class="row-actions">
      {#if avatar}
        <form method="post" action="?/unpicture" use:enhance={inPlace}>
          <button class="quiet small" type="submit" aria-label={m.profile_pictures_remove_avatar()}
            >{m.common_remove()}</button
          >
        </form>
      {/if}
      <button class="quiet small" type="button" popovertarget="picture-avatar" aria-label={act}
        >{avatar ? m.profile_pictures_change() : m.common_add()}</button
      >
    </span>
    <FormDialog id="picture-avatar" title={act} action="?/picture" submit={act} multipart>
      {#snippet intro()}{m.profile_pictures_avatar_intro()}{/snippet}
      <label class="field"
        >{m.profile_pictures_avatar()}
        <input name="file" type="file" accept="image/png,image/jpeg,image/webp" required /></label
      >
    </FormDialog>
  </li>
</ul>
