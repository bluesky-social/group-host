<script lang="ts">
  import { enhance } from '$app/forms';
  import { inPlace } from '#lib/forms.ts';
  import PickMany from '#lib/molecules/PickMany.svelte';
  import { ADDRESS, COUNTRIES, LANGUAGES, languageName, type Place } from '#lib/places.ts';
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
  const policy = $derived(joinPolicy ?? 'approval');
  // A place other than an address (a point on a map) is kept as it is
  // until a town or country is given in its place.
  const address = $derived(location?.$type === ADDRESS ? location : undefined);
  // The town as typed, so the country is asked for once there is one.
  let town = $state('');
  $effect.pre(() => {
    town = address?.locality ?? '';
  });
  // The languages chosen, kept here as they are added and taken off.
  let spoken = $state<string[]>([]);
  $effect.pre(() => {
    spoken = languages.map((l) => l.language);
  });
  // Those offered, and any it has that are not, by name.
  const choices = $derived([
    ...LANGUAGES.map((l) => ({ value: l.tag, name: l.name })),
    ...languages
      .filter((l) => !LANGUAGES.some((o) => o.tag === l.language))
      .map((l) => ({ value: l.language, name: languageName(l.language) }))
  ]);
</script>

<form class="stack" method="post" action="?/profile" use:enhance={inPlace}>
  <label class="field"
    >Name <input name="displayName" required maxlength="64" value={displayName ?? ''} /></label
  >
  <label class="field"
    >What it is for <textarea name="description" maxlength="2000">{description ?? ''}</textarea
    ></label
  >
  <div class="pair">
    <label class="field"
      >Town or city it meets in <input name="town" maxlength="128" bind:value={town} /></label
    >
    <label class="field">
      Country
      <select name="country" value={address?.country ?? ''} required={town.trim() !== ''}>
        <option value="">{town.trim() ? 'Choose a country' : 'No country'}</option>
        {#each COUNTRIES as c (c.code)}<option value={c.code}>{c.name}</option>{/each}
      </select>
    </label>
  </div>
  <input type="hidden" name="online" value="" />
  <label><input type="checkbox" name="online" checked={online} /> It meets online</label>
  <label class="field">
    Topics
    <input
      name="topics"
      maxlength="700"
      value={topics.map((t) => t.topic).join(', ')}
      aria-describedby="topics-hint"
    />
    <span class="mute small" id="topics-hint"
      >Separate up to ten with commas, like climbing, choir or renters' union.</span
    >
  </label>
  <PickMany
    id="languages"
    label="Languages it uses"
    name="language"
    options={choices}
    bind:chosen={spoken}
    max={5}
    most="five"
    placeholder="Type a language"
  />
  <label class="field">
    How people join
    <select name="joinPolicy" value={policy}>
      <option value="approval">An admin approves who joins</option>
      <option value="open">Anyone can join right away</option>
      <option value="invite">Only people who are invited</option>
    </select>
  </label>
  <label class="field">Its own site <input name="url" type="url" value={url ?? ''} /></label>
  <button type="submit">Save</button>
</form>
