<script lang="ts">
  import { enhance } from '$app/forms';
  import { inPlace } from '#lib/forms.ts';
  import Avatar from '#lib/atoms/Avatar.svelte';
  import FormDialog from '#lib/molecules/FormDialog.svelte';
  import { m } from '#lib/paraglide/messages.js';
  /**
   * A group's pictures, to change: its avatar, round beside its name, and
   * its banner, across the top of its page, each as it is now; a new one
   * uploaded from a dialog, or the one there removed, each button named by
   * its verb alone beside the row's name, and in full for a screen reader.
   * `avatar` and
   * `banner` are where they are served, if set; `name`, the group's.
   */
  let { name, avatar, banner }: { name: string; avatar: string | null; banner: string | null } =
    $props();
  const slots = $derived([
    {
      slot: 'avatar',
      name: m.profile_pictures_avatar(),
      src: avatar,
      act: avatar ? m.profile_pictures_change_avatar() : m.profile_pictures_add_avatar(),
      verb: avatar ? m.profile_pictures_change() : m.common_add(),
      remove: m.profile_pictures_remove_avatar(),
      intro: m.profile_pictures_avatar_intro()
    },
    {
      slot: 'banner',
      name: m.profile_pictures_banner(),
      src: banner,
      act: banner ? m.profile_pictures_change_banner() : m.profile_pictures_add_banner(),
      verb: banner ? m.profile_pictures_change() : m.common_add(),
      remove: m.profile_pictures_remove_banner(),
      intro: m.profile_pictures_banner_intro()
    }
  ]);
</script>

<ul class="pictures">
  {#each slots as s (s.slot)}
    <li>
      {#if s.slot === 'avatar'}
        <Avatar src={s.src} size="md" {name} />
      {:else if s.src}
        <img class="banner-thumb" src={s.src} alt="" />
      {:else}
        <span class="banner-thumb" aria-hidden="true"></span>
      {/if}
      <span class="row-main"
        ><strong>{s.name}</strong>{#if !s.src}<span class="what">{m.profile_pictures_none()}</span
          >{/if}</span
      >
      <span class="row-actions">
        {#if s.src}
          <form method="post" action="?/unpicture" use:enhance={inPlace}>
            <input type="hidden" name="slot" value={s.slot} /><button
              class="quiet small"
              type="submit"
              aria-label={s.remove}>{m.common_remove()}</button
            >
          </form>
        {/if}
        <button
          class="quiet small"
          type="button"
          popovertarget={`picture-${s.slot}`}
          aria-label={s.act}>{s.verb}</button
        >
      </span>
      <FormDialog
        id={`picture-${s.slot}`}
        title={s.act}
        action="?/picture"
        submit={s.act}
        multipart
      >
        {#snippet intro()}{s.intro}{/snippet}
        <input type="hidden" name="slot" value={s.slot} />
        <label class="field"
          >{s.name}
          <input name="file" type="file" accept="image/png,image/jpeg,image/webp" required /></label
        >
      </FormDialog>
    </li>
  {/each}
</ul>
