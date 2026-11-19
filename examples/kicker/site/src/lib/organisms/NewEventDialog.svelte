<script>
  import Dialog from '../molecules/Dialog.svelte';
  import { iso, zone } from '../dates.js';
  import { sending } from '../forms.svelte.js';
  /**
   * Publishing an event, for hosts: its button, and the dialog it opens,
   * its name, when, what happens, and a picture of it given to `publish`,
   * the picture apart, to be uploaded; done, the dialog closes.
   */
  let { publish } = $props();
  let open = $state(false);
  const form = sending(async (data) => {
    const event = { name: String(data.get('name')).trim(), startsAt: iso(data.get('startsAt')) };
    const about = String(data.get('description')).trim();
    if (about) event.description = about;
    if (data.get('endsAt')) event.endsAt = iso(data.get('endsAt'));
    const file = data.get('image');
    const picked = file instanceof File && file.size ? file : null;
    if (picked && picked.size > 1_000_000) throw new Error('The picture is over 1 MB.');
    const alt = String(data.get('imageAlt') ?? '').trim();
    if (picked && alt) event.imageAlt = alt;
    await publish(event, picked);
    open = false;
  });
</script>

<button type="button" onclick={() => (open = true)}>New event</button>
<Dialog bind:open label="New event">
  <h2 class="dialog-title">New event</h2>
  <p class="lede">Every member sees it on the calendar, and answers it.</p>
  <form class="form" onsubmit={form.send}>
    <label class="field">Name <input name="name" required maxlength="300" /></label>
    <div class="row">
      <label class="field">Starts <input name="startsAt" type="datetime-local" required /></label>
      <label class="field"
        ><span>Ends <span class="optional">(optional)</span></span>
        <input name="endsAt" type="datetime-local" /></label
      >
    </div>
    <label class="field"
      ><span>What happens <span class="optional">(optional)</span></span>
      <textarea name="description" rows="4" maxlength="3000"></textarea></label
    >
    <label class="field"
      ><span>Poster or photo <span class="optional">(optional, up to 1 MB)</span></span>
      <input name="image" type="file" accept="image/*" /></label
    >
    <label class="field"
      ><span>What it shows <span class="optional">(optional)</span></span>
      <input name="imageAlt" maxlength="300" /></label
    >
    <p class="meta">Times are yours ({zone()}).</p>
    {#if form.failed}<p class="failed" role="status">{form.failed}</p>{/if}
    <div class="acts end">
      <button type="button" class="quiet" onclick={() => (open = false)}>Cancel</button>
      <button type="submit" disabled={form.busy}>Publish</button>
    </div>
  </form>
</Dialog>
