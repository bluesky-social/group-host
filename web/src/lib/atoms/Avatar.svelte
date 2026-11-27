<script lang="ts">
  /**
   * A round picture of a group or a person, or with `app`, an app's icon,
   * square as apps draw theirs; where there is none, or it cannot be
   * drawn, the first letter of `name` on a hue of its own, the same for
   * the same name wherever it is drawn.
   */
  let {
    src,
    size,
    name = '',
    app = false
  }: { src?: string | null; size?: 'sm' | 'md' | 'lg'; name?: string; app?: boolean } = $props();
  let cls = $derived(['av', size, app && 'app'].filter(Boolean).join(' '));
  // Its first letter, past the @ and the did: a handle or DID starts with;
  // drawn by the stylesheet, so it is never read as part of a name beside it.
  const plain = $derived(name.replace(/^@/, '').replace(/^did:[a-z]+:/, ''));
  const initial = $derived([...plain][0]?.toUpperCase() ?? '');
  const hue = $derived([...name].reduce((h, c) => (h * 31 + (c.codePointAt(0) ?? 0)) % 360, 7));
  // A picture that did not load, as an app's icon the host could not read.
  let failed = $state<string | null>(null);
</script>

{#if src && failed !== src}<img
    class={cls}
    {src}
    alt=""
    onerror={() => (failed = src ?? null)}
  />{:else}<span class={cls} style:--hue={hue} data-initial={initial} aria-hidden="true"
  ></span>{/if}
