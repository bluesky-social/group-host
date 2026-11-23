<script lang="ts">
  import { m } from '#lib/paraglide/messages.js';
  import Avatar from '#lib/atoms/Avatar.svelte';
  import { type Actor, findActors } from '#lib/typeahead.ts';
  /**
   * A field for someone's handle, labelled `label`, with whoever on the
   * Atmosphere matches what is typed so far dropping down over what is
   * under it. Picking one fills it in, then calls `onpick` with the field
   * if one is given, to go on as the form's button would. One's own handle,
   * to sign in with, is `autocomplete` username; anyone else's is off, so
   * the browser offers no saved sign-in for it.
   */
  let {
    id,
    label,
    name = 'handle',
    placeholder,
    autocomplete = 'off',
    value = $bindable(''),
    onpick
  }: {
    id: string;
    label: string;
    name?: string;
    placeholder?: string;
    autocomplete?: 'username' | 'off';
    value?: string;
    onpick?: (field: HTMLInputElement) => void;
  } = $props();
  let field = $state<HTMLInputElement>();
  let found = $state<Actor[]>([]);
  let open = $state(false);
  // Which one Enter picks: none till the arrows choose one, so Enter sends what is typed.
  let at = $state(-1);
  const list = $derived(`${id}-list`);
  // Looked up a moment after typing stops; a lookup overtaken by more typing is let go.
  $effect(() => {
    const q = value;
    const ask = new AbortController();
    const wait = setTimeout(() => {
      findActors(q, ask.signal)
        .then((a) => {
          found = a;
          at = -1;
        })
        .catch(() => {});
    }, 150);
    return () => {
      clearTimeout(wait);
      ask.abort();
    };
  });
  const showing = $derived(open && found.length > 0);
  function pick(a: Actor) {
    value = a.handle;
    close();
    if (!field) return;
    // The field holds it now, not only once the page is next drawn, so a form sent at once sends it.
    field.value = a.handle;
    onpick?.(field);
  }
  function press(e: MouseEvent, a: Actor) {
    // Picked on the press, before the field loses focus and the list closes.
    e.preventDefault();
    pick(a);
  }
  // What the arrows chose was among what matched before; typing or closing lets it go.
  function typed() {
    open = true;
    at = -1;
  }
  function close() {
    open = false;
    at = -1;
  }
  function keydown(e: KeyboardEvent) {
    if (!showing) return;
    if (e.key === 'ArrowDown' || e.key === 'ArrowUp') {
      e.preventDefault();
      const n = found.length;
      at = e.key === 'ArrowDown' ? (at + 1) % n : (at - 1 + n) % n;
    } else if (e.key === 'Enter' && found[at]) {
      e.preventDefault();
      pick(found[at]);
    } else if (e.key === 'Escape') {
      // Closes the list, not the dialog around it.
      e.preventDefault();
      e.stopPropagation();
      close();
    }
  }
</script>

<div class="field handle-input">
  <label for={id}>{label}</label>
  <input
    bind:this={field}
    {id}
    {name}
    {placeholder}
    {autocomplete}
    autocapitalize="none"
    spellcheck="false"
    required
    role="combobox"
    aria-expanded={showing}
    aria-controls={list}
    aria-autocomplete="list"
    aria-activedescendant={showing && at >= 0 ? `${list}-${at}` : undefined}
    bind:value
    oninput={typed}
    onfocus={() => (open = true)}
    onblur={close}
    onkeydown={keydown}
  />
  {#if showing}
    <!-- Over what is under the field, rather than pushing it down. -->
    <div id={list} class="drop-list" role="listbox" aria-label={m.handle_input_accounts()}>
      {#each found as a, i (a.did)}
        <div
          id="{list}-{i}"
          class="drop-option"
          role="option"
          tabindex="-1"
          aria-selected={i === at}
          onmousedown={(e) => press(e, a)}
          onmouseenter={() => (at = i)}
        >
          <Avatar src={a.avatar} name={a.handle} />
          <span class="who">
            {#if a.displayName}<strong>{a.displayName}</strong>{/if}
            <span class="mute">@{a.handle}</span>
          </span>
        </div>
      {/each}
    </div>
  {/if}
</div>
