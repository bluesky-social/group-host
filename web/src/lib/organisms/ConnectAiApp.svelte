<script lang="ts">
  import Copy from '#lib/atoms/Copy.svelte';
  import { m } from '#lib/paraglide/messages.js';
  /**
   * How to have an AI app act for the person in the group, in three
   * numbered steps: add the address (`mcp`) to it, sign in here and
   * approve it, then ask it things, each a thing the host's tools do.
   */
  let { name, mcp }: { name: string; mcp: string } = $props();
  const asks = [
    m.connect_ai_app_ask_requests(),
    m.connect_ai_app_ask_invite({ handle: 'dana.bsky.social' }),
    m.connect_ai_app_ask_queue()
  ];
</script>

<ol class="connect">
  <li>
    <span class="ordinal">1</span>
    <div class="step">
      <strong>{m.connect_ai_app_add()}</strong>
      <div class="copyable"><code>{mcp}</code><Copy text={mcp} /></div>
    </div>
  </li>
  <li>
    <span class="ordinal">2</span>
    <div class="step">
      <strong>{m.connect_ai_app_approve()}</strong>
      <span class="what">{m.connect_ai_app_acts({ group: name })}</span>
    </div>
  </li>
  <li>
    <span class="ordinal">3</span>
    <div class="step">
      <strong>{m.connect_ai_app_ask()}</strong>
      <ul class="asks">
        {#each asks as ask (ask)}<li>{ask}</li>{/each}
      </ul>
    </div>
  </li>
</ol>
