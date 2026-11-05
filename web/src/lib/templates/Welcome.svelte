<script lang="ts">
  import type { Snippet } from 'svelte';
  import SignIn from '#lib/molecules/SignIn.svelte';
  /**
   * A page for someone not signed in, whose one way on is signing in: its
   * heading (`handle`, when it is a group's handle, set smaller and broken
   * anywhere), a line on what signing in is for, and the button.
   */
  let {
    title,
    handle = false,
    children
  }: { title: string; handle?: boolean; children: Snippet } = $props();
</script>

<div class="welcome">
  <h1 class:handle>{title}</h1>
  <p>{@render children()}</p>
  <SignIn />
</div>
