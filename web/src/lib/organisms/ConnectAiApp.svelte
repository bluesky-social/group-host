<script lang="ts">
  import Copy from '#lib/atoms/Copy.svelte';
  /**
   * How to have an AI app act for the person in the group, in three
   * numbered steps: add the address (`mcp`) to it, sign in here and
   * approve it, then ask it things, each a thing the host's tools do.
   */
  let { name, mcp }: { name: string; mcp: string } = $props();
  const asks = [
    'Who is asking to join, and what did they say?',
    'Invite dana.bsky.social',
    'What is waiting in the moderation queue?'
  ];
</script>

<ol class="connect">
  <li>
    <span class="ordinal">1</span>
    <div class="step">
      <strong>Add this address to your AI app as a connector</strong>
      <div class="copyable"><code>{mcp}</code><Copy text={mcp} /></div>
    </div>
  </li>
  <li>
    <span class="ordinal">2</span>
    <div class="step">
      <strong>When it asks, sign in here and approve it</strong>
      <span class="what">It acts for you in {name}, with what your roles let you do.</span>
    </div>
  </li>
  <li>
    <span class="ordinal">3</span>
    <div class="step">
      <strong>Ask it things like</strong>
      <ul class="asks">
        {#each asks as ask (ask)}<li>{ask}</li>{/each}
      </ul>
    </div>
  </li>
</ol>
