<script lang="ts">
  import { m } from '#lib/paraglide/messages.js';
  /** A button that copies `text` for pasting elsewhere, and says so once it has. */
  let { text }: { text: string } = $props();
  let copied = $state(false);
  async function copy() {
    await navigator.clipboard.writeText(text);
    copied = true;
  }
</script>

<button class="quiet" type="button" onclick={copy}
  >{copied ? m.copy_copied() : m.copy_copy()}</button
>
