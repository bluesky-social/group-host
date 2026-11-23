<script lang="ts">
  import { m } from '#lib/paraglide/messages.js';
  import { tick } from 'svelte';
  /**
   * A field for choosing a few of many, labelled `label`: what is chosen,
   * each a button that takes it off, over a field to type into, with what
   * matches dropping down to pick from, once it is clicked, typed into
   * or arrowed down, never only for having focus. Picking one clears the
   * field and keeps the list open for the next; Enter picks, never sends
   * the form.
   * The form is sent each chosen as `name`, after a blank one, so a form
   * with none chosen still says so. Under it, how many it takes; at
   * `max`, the field goes, and the hint says why. `optional`, it says so
   * after its label, as a field the form can be sent without.
   */
  let {
    id,
    label,
    name,
    options,
    chosen = $bindable([]),
    max,
    placeholder,
    most,
    optional = false
  }: {
    id: string;
    label: string;
    name: string;
    options: { value: string; name: string }[];
    chosen?: string[];
    max: number;
    placeholder: string;
    /** How many at most, in words, for the hint under it: "five". */
    most: string;
    optional?: boolean;
  } = $props();
  let field = $state<HTMLInputElement>();
  let typed = $state('');
  let open = $state(false);
  // Which one Enter picks: the first that matches, till the arrows choose another.
  let at = $state(0);
  // What was last added or taken off, said to a screen reader.
  let said = $state('');
  const list = $derived(`${id}-list`);
  const nameOf = (value: string) => options.find((o) => o.value === value)?.name ?? value;
  const found = $derived.by(() => {
    const q = typed.trim().toLowerCase();
    const left = options.filter((o) => !chosen.includes(o.value));
    if (!q) return left;
    // Those that start with what is typed, then those that have it anywhere.
    const starts = left.filter(
      (o) => o.name.toLowerCase().startsWith(q) || o.value.toLowerCase() === q
    );
    return [
      ...starts,
      ...left.filter((o) => !starts.includes(o) && o.name.toLowerCase().includes(q))
    ];
  });
  const full = $derived(chosen.length >= max);
  const showing = $derived(open && !full && found.length > 0);
  function pick(value: string) {
    chosen = [...chosen, value];
    said = m.pick_many_added({ name: nameOf(value) });
    typed = '';
    at = 0;
  }
  function drop(value: string) {
    chosen = chosen.filter((v) => v !== value);
    said = m.pick_many_removed({ name: nameOf(value) });
    // The field, not the page's top, once the button pressed is gone.
    tick().then(() => field?.focus());
  }
  function keydown(e: KeyboardEvent) {
    if (e.key === 'Enter') {
      // Never sends the form from here: picks what is chosen in the list, if anything.
      e.preventDefault();
      if (showing && found[at]) pick(found[at].value);
    } else if (e.key === 'ArrowDown' || e.key === 'ArrowUp') {
      e.preventDefault();
      if (!open) return void (open = true);
      const n = found.length;
      if (n) at = e.key === 'ArrowDown' ? (at + 1) % n : (at - 1 + n) % n;
    } else if (e.key === 'Escape' && showing) {
      // Closes the list, not the dialog around it.
      e.preventDefault();
      e.stopPropagation();
      open = false;
    }
  }
  function press(e: MouseEvent, value: string) {
    // Picked on the press, before the field loses focus and the list closes.
    e.preventDefault();
    pick(value);
  }
  // The one chosen by the arrows is kept in sight as the list scrolls.
  $effect(() => {
    if (showing) document.getElementById(`${list}-${at}`)?.scrollIntoView({ block: 'nearest' });
  });
</script>

<div class="field" role="group" aria-labelledby="{id}-label">
  <label id="{id}-label" for={id}
    >{label}{#if optional}{' '}<span class="optional">{m.common_optional()}</span>{/if}</label
  >
  <div class="pick-many">
    <input type="hidden" {name} value="" />
    {#each chosen as value (value)}<input type="hidden" {name} {value} />{/each}
    {#if chosen.length}
      <ul class="chosen" aria-label={m.pick_many_chosen()}>
        {#each chosen as value (value)}
          <li>
            <button
              class="chip"
              type="button"
              aria-label={m.pick_many_remove({ name: nameOf(value) })}
              onclick={() => drop(value)}>{nameOf(value)}<span aria-hidden="true">×</span></button
            >
          </li>
        {/each}
      </ul>
    {/if}
    {#if !full}
      <input
        bind:this={field}
        {id}
        {placeholder}
        autocomplete="off"
        role="combobox"
        aria-expanded={showing}
        aria-controls={list}
        aria-autocomplete="list"
        aria-describedby="{id}-hint"
        aria-activedescendant={showing ? `${list}-${at}` : undefined}
        bind:value={typed}
        oninput={() => ((open = true), (at = 0))}
        onclick={() => (open = true)}
        onblur={() => (open = false)}
        onkeydown={keydown}
      />
    {/if}
    {#if showing}
      <!-- Over what is under the field, rather than pushing it down. -->
      <div id={list} class="drop-list" role="listbox" aria-label={label}>
        {#each found as o, i (o.value)}
          <div
            id="{list}-{i}"
            class="drop-option"
            role="option"
            tabindex="-1"
            aria-selected={i === at}
            onmousedown={(e) => press(e, o.value)}
            onmouseenter={() => (at = i)}
          >
            {o.name}
          </div>
        {/each}
      </div>
    {/if}
  </div>
  <span class="mute small" id="{id}-hint"
    >{full ? m.pick_many_full({ most }) : m.pick_many_up_to({ most })}</span
  >
  <span class="sr-only" aria-live="polite">{said}</span>
</div>
