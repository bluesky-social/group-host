<script lang="ts">
  import { m } from '#lib/paraglide/messages.js';
  /** A group's rules, folded under how many there are, each in order: its title, and under it what it says. */
  let { rules }: { rules: { rkey: string; title: string; text?: string }[] } = $props();
</script>

{#if rules.length}
  <details class="rules">
    <summary>{m.rules_title()}<span class="count">{rules.length}</span></summary>
    <ol>
      {#each rules as r (r.rkey)}
        <li>
          <strong>{r.title}</strong>{#if r.text}<span class="what">{r.text}</span>{/if}
        </li>
      {/each}
    </ol>
  </details>
{/if}
