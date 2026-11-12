<script lang="ts">
  import Copy from '#lib/atoms/Copy.svelte';
  import ConfirmDialog from '#lib/molecules/ConfirmDialog.svelte';
  import FormDialog from '#lib/molecules/FormDialog.svelte';
  /**
   * A group's handle: its address here, or a domain of its own, with
   * whether that domain points to it still (`pointing`), to go back from;
   * and the dialog taking a domain, opened by a button whose
   * `popovertarget` is `use-domain`: the domain, then the DNS record that
   * points it to the group (`did`).
   */
  let {
    did,
    handle,
    address,
    pointing
  }: { did: string; handle: string; address: string; pointing: boolean | null } = $props();
  const own = $derived(handle !== address);
  const record = $derived(`did=${did}`);
  // The domain being typed, named in the record to add as it is.
  let domain = $state('');
</script>

<ul class="handles">
  <li>
    <span class="row-main">
      <strong class="mono">{handle}</strong>
      <span class="what"
        >{own
          ? pointing === false
            ? "This domain no longer points to the group. Fix its _atproto record, or go back to the group's address."
            : "The group's own domain."
          : 'The address this host gave it.'}</span
      >
    </span>
    {#if own}
      <span class="row-actions"
        ><button
          class="quiet small"
          type="button"
          popovertarget="unhandle"
          aria-label="Go back to the group's address">Go back</button
        ></span
      >
      <ConfirmDialog
        id="unhandle"
        title="Go back to the group's address?"
        action="?/unhandle"
        confirm="Go back"
        danger={false}
        fields={{ address }}
      >
        The group's handle becomes <span class="mono">{address}</span> again in every app, and
        <span class="mono">{handle}</span> stops being its handle.
      </ConfirmDialog>
    {/if}
  </li>
</ul>
<FormDialog id="use-domain" title="Use your own domain" action="?/handle" submit="Use">
  {#snippet intro()}Type your domain, then add this record where you manage its DNS. New records can
    take a few minutes to show up.{/snippet}
  <label class="field"
    >Domain <input
      name="domain"
      required
      maxlength="253"
      placeholder="riders.org"
      bind:value={domain}
    /></label
  >
  <dl class="record">
    <dt>Type</dt>
    <dd><code>TXT</code></dd>
    <dt>Name</dt>
    <dd>
      <code>_atproto.{domain.trim().toLowerCase()}</code>{#if !domain.trim()}<span class="mute"
          >your domain</span
        >{/if}
    </dd>
  </dl>
  <div class="field">
    <span>Value</span>
    <div class="copyable"><code>{record}</code><Copy text={record} /></div>
  </div>
</FormDialog>
