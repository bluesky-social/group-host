<script lang="ts">
  /** A button that copies `text` for pasting elsewhere, and says so once it has. */
  let { text }: { text: string } = $props();
  let copied = $state(false);
  async function copy() {
    await navigator.clipboard.writeText(text);
    copied = true;
  }
</script>

<button class="quiet" type="button" onclick={copy}>{copied ? 'Copied' : 'Copy'}</button>
