<script lang="ts">
  import Copy from '#lib/atoms/Copy.svelte';
  import ConfirmDialog from '#lib/molecules/ConfirmDialog.svelte';
  import FormDialog from '#lib/molecules/FormDialog.svelte';
  import { m } from '#lib/paraglide/messages.js';
  import { ParaglideMessage } from '@inlang/paraglide-js-svelte';
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
            ? m.group_handle_not_pointing()
            : m.group_handle_own()
          : m.group_handle_given()}</span
      >
    </span>
    {#if own}
      <span class="row-actions"
        ><button
          class="quiet small"
          type="button"
          popovertarget="unhandle"
          aria-label={m.group_handle_back_title()}>{m.group_handle_back()}</button
        ></span
      >
      <ConfirmDialog
        id="unhandle"
        title={m.group_handle_back_title()}
        action="?/unhandle"
        confirm={m.group_handle_back()}
        danger={false}
        fields={{ address }}
      >
        <ParaglideMessage message={m.group_handle_back_body}>
          {#snippet given()}<span class="mono">{address}</span>{/snippet}
          {#snippet current()}<span class="mono">{handle}</span>{/snippet}
        </ParaglideMessage>
      </ConfirmDialog>
    {/if}
  </li>
</ul>
<FormDialog
  id="use-domain"
  title={m.group_handle_domain_title()}
  action="?/handle"
  submit={m.group_handle_use()}
>
  {#snippet intro()}{m.group_handle_domain_intro()}{/snippet}
  <label class="field"
    >{m.group_handle_domain()}
    <input
      name="domain"
      required
      maxlength="253"
      placeholder="riders.org"
      bind:value={domain}
    /></label
  >
  <dl class="record">
    <dt>{m.group_handle_record_type()}</dt>
    <dd><code>TXT</code></dd>
    <dt>{m.group_handle_record_name()}</dt>
    <dd>
      <code>_atproto.{domain.trim().toLowerCase()}</code>{#if !domain.trim()}<span class="mute"
          >{m.group_handle_your_domain()}</span
        >{/if}
    </dd>
  </dl>
  <div class="field">
    <span>{m.group_handle_record_value()}</span>
    <div class="copyable"><code>{record}</code><Copy text={record} /></div>
  </div>
</FormDialog>
