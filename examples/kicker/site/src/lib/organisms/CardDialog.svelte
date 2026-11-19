<script>
  import Dialog from '../molecules/Dialog.svelte';
  import { sending } from '../forms.svelte.js';
  /**
   * The viewer's card: its button, and the dialog it opens, filled with
   * the card as it is (`card`): its words, given to `save` with a picture
   * picked to put in place of the one it has, if any, and whether to drop
   * the one it has; done, the dialog closes.
   */
  let { card, save } = $props();
  let open = $state(false);
  /** The picture picked and the box to take it off: unlike the words, empty each time it opens. */
  let picker = $state();
  let dropping = $state(false);
  const edit = () => {
    if (picker) picker.value = '';
    dropping = false;
    open = true;
  };
  const form = sending(
    async (data) => {
      const file = data.get('avatar');
      const picked = file instanceof File && file.size ? file : null;
      if (picked && picked.size > 1_000_000) throw new Error('The picture is over 1 MB.');
      await save(
        { about: String(data.get('about')).trim(), offers: String(data.get('offers')).trim() },
        picked,
        dropping
      );
      open = false;
    },
    { keep: true }
  );
</script>

<button type="button" onclick={edit}>Edit your card</button>
<Dialog bind:open label="Edit your card">
  <h2 class="dialog-title">Edit your card</h2>
  <p class="lede">What the place's members see beside your name.</p>
  <form class="form" onsubmit={form.send}>
    <label class="field"
      ><span>About <span class="optional">(optional)</span></span>
      <input name="about" maxlength="300" value={card?.about ?? ''} /></label
    >
    <label class="field"
      ><span>What you can help with <span class="optional">(optional)</span></span>
      <input name="offers" maxlength="300" value={card?.offers ?? ''} /></label
    >
    <label class="field"
      ><span
        >{card?.avatar ? 'A new picture' : 'Your picture'}
        <span class="optional">(optional, up to 1 MB)</span></span
      >
      <input name="avatar" type="file" accept="image/*" bind:this={picker} /></label
    >
    {#if card?.avatar}<label class="check"
        ><input type="checkbox" bind:checked={dropping} /> Take my picture off</label
      >{/if}
    {#if form.failed}<p class="failed" role="status">{form.failed}</p>{/if}
    <div class="acts end">
      <button type="button" class="quiet" onclick={() => (open = false)}>Cancel</button>
      <button type="submit" disabled={form.busy}>Save</button>
    </div>
  </form>
</Dialog>
