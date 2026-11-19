<script>
  /**
   * The place's front: its name as the masthead, filling the width of its
   * band; under it, its line, when and where, who gets in, and across from
   * them the way in (`actions`, the one act that counts first). `note`, in
   * place of who gets in, to someone signed in but not let in yet.
   * `about`: the place's public face.
   */
  let { about, note, actions } = $props();
</script>

<header class="band tall">
  <h1 class="masthead" style:--letters={about.name.length}>{about.name}</h1>
</header>
<div class="page landing">
  <div class="landing-text">
    <p class="landing-line">{about.line}</p>
    <p>{about.hours}<br />{about.address}</p>
    {#if note}<div class="landing-note">{@render note()}</div>{:else}<p class="landing-note">
        Members only. Sign in with your Atmosphere account, and an admin lets you in.
      </p>{/if}
  </div>
  <div class="landing-acts">{@render actions()}</div>
</div>
