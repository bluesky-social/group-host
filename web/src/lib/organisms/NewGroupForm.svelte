<script lang="ts">
  import { onMount } from 'svelte';
  import { finished, labelOf } from '#lib/handles.ts';
  /**
   * Starting a group: its name, and its address under the host's
   * `domain`, made from the name as it is typed until it is changed by
   * hand; whether that address is free, asked once typing stops, with the
   * next free one to take if not; its words; and how people get in.
   * `failed` is what the host said of the last try, if it refused it.
   */
  let { failed, domain }: { failed?: string; domain: string } = $props();
  let name = $state('');
  let label = $state('');
  let edited = $state(false);
  let address: HTMLInputElement;
  // An address typed before the page's script ran was typed by hand all the same.
  onMount(() => {
    if (!address.value) return;
    edited = true;
    label = labelOf(address.value);
  });
  // Taken, and the next free one if there is one near: none while free or unasked.
  let taken = $state<{ label: string; next?: string } | null>(null);

  // Free unless the host says not: its handle, or kept as a group's
  // address. A host that cannot say leaves it to the host to refuse.
  const free = async (l: string) => {
    const asked = await fetch(
      `/xrpc/fyi.opensocial.checkAddress?address=${encodeURIComponent(l + domain)}`
    ).catch(() => null);
    const said: { available?: boolean } | null = asked?.ok ? await asked.json() : null;
    return said?.available !== false;
  };
  $effect(() => {
    const l = finished(label);
    taken = null;
    if (!l) return;
    const wait = setTimeout(async () => {
      if (await free(l)) return;
      let next: string | undefined;
      for (let n = 2; n < 20 && !next; n++) if (await free(`${l}-${n}`)) next = `${l}-${n}`;
      if (finished(label) === l) taken = { label: l, next };
    }, 350);
    return () => clearTimeout(wait);
  });
</script>

<form class="stack" method="post">
  {#if failed}<p class="danger">{failed}</p>{/if}
  <label class="field">
    Name
    <input
      name="displayName"
      required
      maxlength="64"
      bind:value={name}
      oninput={() => (label = edited ? label : labelOf(name))}
    />
  </label>
  <label class="field">
    Its address
    <span class="address">
      <input
        name="label"
        required
        maxlength="63"
        pattern="[a-z0-9][a-z0-9\-]*"
        bind:this={address}
        bind:value={label}
        oninput={() => ((edited = true), (label = labelOf(label)))}
      />
      <span class="mute mono">{domain}</span>
    </span>
  </label>
  {#if taken}
    <p class="danger">
      {taken.label}{domain} is taken.
      {#if taken.next}<button
          class="quiet"
          type="button"
          onclick={() => ((edited = true), (label = taken?.next ?? label))}
          >Use {taken.next}{domain}</button
        >{/if}
    </p>
  {:else}
    <p class="mute small">This is the group's handle and web address. You can change it.</p>
  {/if}
  <label class="field"
    >What it is for <textarea name="description" maxlength="2000"></textarea></label
  >
  <label class="field">
    How people join
    <select name="joinPolicy">
      <option value="approval">People ask to join, and an admin approves them</option>
      <option value="open">Anyone can join right away</option>
      <option value="invite">Only people who are invited</option>
    </select>
  </label>
  <button type="submit">Start the group</button>
</form>
