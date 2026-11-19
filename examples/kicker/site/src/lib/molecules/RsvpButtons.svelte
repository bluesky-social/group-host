<script>
  /**
   * The viewer's answer to an event, `mine` (going, interested, or none),
   * as two buttons, the one given pressed; pressing it again takes it back.
   * `answer` is told the answer chosen, `none` to take it back.
   */
  let { mine, answer } = $props();
  let busy = $state(false);
  const press = async (status) => {
    busy = true;
    try {
      await answer(mine === status ? 'none' : status);
    } finally {
      busy = false;
    }
  };
</script>

<!-- Going acts, in the colour that acts; being interested is quieter. -->
{#each [['going', 'Going', ''], ['interested', 'Interested', 'quiet']] as [status, label, kind] (status)}
  <button
    type="button"
    class={kind}
    aria-pressed={mine === status}
    disabled={busy}
    onclick={() => press(status)}>{label}</button
  >
{/each}
