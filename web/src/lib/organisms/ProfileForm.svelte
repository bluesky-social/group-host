<script lang="ts">
  import { enhance } from '$app/forms';
  import PickMany from '#lib/molecules/PickMany.svelte';
  import SaveBar from '#lib/molecules/SaveBar.svelte';
  import { Saving } from '#lib/saving.svelte.ts';
  import { m } from '#lib/paraglide/messages.js';
  import { ADDRESS, countries, languageName, offeredLanguages, type Place } from '#lib/places.ts';
  /**
   * A group's profile, to change: its name, its words, where it meets
   * and whether online, what it is about, the languages it uses, how
   * people join, and its own site. Its links are a list of their own
   * (LinksEditor).
   */
  let {
    displayName,
    description,
    joinPolicy,
    url,
    location,
    online = false,
    topics = [],
    languages = []
  }: {
    displayName?: string;
    description?: string;
    joinPolicy?: string;
    url?: string;
    location?: Place;
    online?: boolean;
    topics?: { topic: string }[];
    languages?: { language: string }[];
  } = $props();
  const saving = new Saving();
  const policy = $derived(joinPolicy ?? 'approval');
  // A place other than an address (a point on a map) is kept as it is
  // until a town or country is given in its place.
  const address = $derived(location?.$type === ADDRESS ? location : undefined);
  // The town as typed, so the country is asked for once there is one: the
  // saved one, drawn by the server, until typed over (even before the page
  // hydrates), and the saved one again once a save changes it.
  let town = $derived(address?.locality ?? '');
  // The languages chosen, kept here as they are added and taken off.
  let spoken = $state<string[]>([]);
  $effect.pre(() => {
    spoken = languages.map((l) => l.language);
  });
  // Those offered, and any it has that are not, by name.
  const offered = offeredLanguages();
  const choices = $derived([
    ...offered.map((l) => ({ value: l.tag, name: l.name })),
    ...languages
      .filter((l) => !offered.some((o) => o.tag === l.language))
      .map((l) => ({ value: l.language, name: languageName(l.language) }))
  ]);
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
  <div class="pair">
    <label class="field"
      ><span>{m.profile_form_town()} <span class="optional">{m.common_optional()}</span></span>
      <input name="town" maxlength="128" bind:value={town} /></label
    >
    <label class="field">
      {m.profile_form_country()}
      <select name="country" defaultValue={address?.country ?? ''} required={town.trim() !== ''}>
        <option value=""
          >{town.trim() ? m.profile_form_country_choose() : m.profile_form_country_none()}</option
        >
        {#each countries() as c (c.code)}<option value={c.code}>{c.name}</option>{/each}
      </select>
    </label>
  </div>
  <input type="hidden" name="online" value="" />
  <label><input type="checkbox" name="online" checked={online} /> {m.profile_form_online()}</label>
  <label class="field">
    <span>{m.profile_form_topics()} <span class="optional">{m.common_optional()}</span></span>
    <input
      name="topics"
      maxlength="700"
      value={topics.map((t) => t.topic).join(', ')}
      aria-describedby="topics-hint"
    />
    <span class="mute small" id="topics-hint">{m.profile_form_topics_hint()}</span>
  </label>
  <PickMany
    id="languages"
    label={m.profile_form_languages()}
    name="language"
    options={choices}
    bind:chosen={spoken}
    max={5}
    most={m.profile_form_languages_most()}
    placeholder={m.profile_form_languages_placeholder()}
    optional
  />
  <label class="field">
    {m.profile_form_join()}
    <select name="joinPolicy" defaultValue={policy}>
      <option value="approval">{m.profile_form_join_approval()}</option>
      <option value="open">{m.profile_form_join_open()}</option>
      <option value="invite">{m.profile_form_join_invite()}</option>
    </select>
  </label>
  <label class="field"
    ><span>{m.profile_form_url()} <span class="optional">{m.common_optional()}</span></span>
    <input name="url" type="url" value={url ?? ''} /></label
  >
  <SaveBar {saving} what={m.profile_form_saves()} />
</form>
