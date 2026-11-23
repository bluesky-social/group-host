<script lang="ts">
  import { enhance } from '$app/forms';
  import { inPlace } from '#lib/forms.ts';
  import { m } from '#lib/paraglide/messages.js';
  /**
   * How the person looking gets into the group, as its join policy says
   * and as they stand: in at once by an invite, whatever the policy,
   * or where anyone can join, an invite declined as it is on the home
   * page, asked nothing as another can be sent; asking, with a note, where someone looks at
   * each request, and then waiting, which they may cancel; or told to
   * ask for an invite. Once `joined`, a welcome.
   */
  let {
    name,
    joinPolicy,
    asked = false,
    invited = false,
    joined = false
  }: {
    name: string;
    joinPolicy?: string;
    asked?: boolean;
    invited?: boolean;
    joined?: boolean;
  } = $props();
  const policy = $derived(joinPolicy ?? 'approval');
</script>

<section class="card stack join">
  {#if joined}
    <h2>{m.join_panel_joined_title()}</h2>
    <p class="mute">{m.join_panel_joined_body({ group: name })}</p>
  {:else if invited}
    <h2>{m.join_panel_invited_title()}</h2>
    <p class="mute">{m.join_panel_invited_body({ group: name })}</p>
    <div class="choices">
      <form method="post" action="?/decline" use:enhance={inPlace}>
        <button class="quiet" type="submit">{m.common_decline()}</button>
      </form>
      <form method="post" action="?/join" use:enhance={inPlace}>
        <button type="submit">{m.common_join()}</button>
      </form>
    </div>
  {:else if asked}
    <h2>{m.join_panel_asked_title()}</h2>
    <p class="mute">{m.join_panel_asked_body()}</p>
    <form method="post" action="?/cancel" use:enhance={inPlace}>
      <button class="quiet" type="submit">{m.join_panel_cancel()}</button>
    </form>
  {:else if policy === 'open'}
    <h2>{m.join_panel_open_title({ group: name })}</h2>
    <p class="mute">{m.join_panel_open_body()}</p>
    <form method="post" action="?/join" use:enhance={inPlace}>
      <button type="submit">{m.common_join()}</button>
    </form>
  {:else if policy === 'invite'}
    <h2>{m.join_panel_invite_title()}</h2>
    <p class="mute">{m.join_panel_invite_body({ group: name })}</p>
  {:else}
    <h2>{m.join_panel_ask_title()}</h2>
    <p class="mute">{m.join_panel_ask_body({ group: name })}</p>
    <form class="stack" method="post" action="?/join" use:enhance={inPlace}>
      <label class="field"
        ><span>{m.join_panel_note()} <span class="optional">{m.common_optional()}</span></span>
        <textarea name="message" rows="3" maxlength="2000"></textarea></label
      >
      <button type="submit">{m.join_panel_send()}</button>
    </form>
  {/if}
</section>
