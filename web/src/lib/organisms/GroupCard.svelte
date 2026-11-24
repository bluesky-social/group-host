<script lang="ts">
  import type { Snippet } from 'svelte';
  import Avatar from '#lib/atoms/Avatar.svelte';
  import Rules from '#lib/molecules/Rules.svelte';
  /**
   * A group as its profile shows it: its banner, when it has one; its
   * picture (over the banner's foot), its name and handle beside it, and
   * `nav`, the ways to its other pages, across from them; its words; its
   * rules; then `children`, where the person looking stands
   * with it. With no `name`, its profile is not theirs to see, and it is
   * its handle alone.
   */
  let {
    handle,
    name,
    description,
    avatar,
    banner,
    rules = [],
    nav,
    children
  }: {
    handle: string;
    name?: string;
    description?: string;
    avatar?: string | null;
    banner?: string | null;
    rules?: { rkey: string; title: string; text?: string }[];
    nav?: Snippet;
    children?: Snippet;
  } = $props();
</script>

<section class="group-card">
  {#if name && banner}<img class="banner" src={banner} alt="" />{/if}
  <div class="group-card-body">
    {#if name}
      <div class="identity">
        <Avatar src={avatar} size="lg" {name} />
        <div class="identity-name">
          <h1>{name}</h1>
          <p class="mute mono">{handle}</p>
        </div>
        {@render nav?.()}
      </div>
      {#if description}<p class="mute description">{description}</p>{/if}
      <Rules {rules} />
    {:else}
      <h1 class="handle">{handle}</h1>
      {@render nav?.()}
    {/if}
    {@render children?.()}
  </div>
</section>
