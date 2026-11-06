<script lang="ts">
  /**
   * A line icon, by name: the ones an account's PDS draws beside what an
   * app asks for (Lucide's, ISC; Bluesky's butterfly, filled). Drawn in
   * the text's colour by `.icon`, `lg` beside a row's title.
   */
  let {
    name,
    lg = false
  }: {
    name:
      | 'mail'
      | 'id-card'
      | 'user'
      | 'bluesky'
      | 'chat'
      | 'set'
      | 'book'
      | 'badge'
      | 'users'
      | 'check'
      | 'more';
    lg?: boolean;
  } = $props();
</script>

<svg
  class="icon"
  class:icon-lg={lg}
  class:icon-filled={name === 'bluesky'}
  viewBox="0 0 24 24"
  aria-hidden="true"
>
  {#if name === 'mail'}
    <rect width="20" height="16" x="2" y="4" rx="2" /><path
      d="m22 7-8.97 5.7a1.94 1.94 0 0 1-2.06 0L2 7"
    />
  {:else if name === 'id-card'}
    <path d="M16 10h2" /><path d="M16 14h2" /><path d="M6.17 15a3 3 0 0 1 5.66 0" /><circle
      cx="9"
      cy="11"
      r="2"
    /><rect x="2" y="5" width="20" height="14" rx="2" />
  {:else if name === 'user'}
    <path d="M19 21v-2a4 4 0 0 0-4-4H9a4 4 0 0 0-4 4v2" /><circle cx="12" cy="7" r="4" />
  {:else if name === 'bluesky'}
    <path
      d="M6.335 4.212c2.293 1.76 4.76 5.327 5.665 7.241.906-1.914 3.372-5.482 5.665-7.241C19.319 2.942 22 1.960 22 5.086c0 .624-.35 5.244-.556 5.994-.713 2.608-3.315 3.273-5.629 2.870 4.045.704 5.074 3.035 2.852 5.366-4.220 4.426-6.066-1.111-6.540-2.530-.086-.260-.126-.382-.127-.278 0-.104-.041.018-.128.278-.473 1.419-2.318 6.956-6.539 2.530-2.222-2.331-1.193-4.662 2.852-5.366-2.314.403-4.916-.262-5.630-2.870C2.350 10.330 2 5.710 2 5.086c0-3.126 2.680-2.144 4.335-.874Z"
    />
  {:else if name === 'chat'}
    <path d="M7.9 20A9 9 0 1 0 4 16.1L2 22Z" /><path d="M8 12h.01" /><path d="M12 12h.01" /><path
      d="M16 12h.01"
    />
  {:else if name === 'set'}
    <circle cx="12" cy="12" r="1" /><path
      d="M20.2 20.2c2.04-2.03.02-7.36-4.5-11.9-4.54-4.52-9.87-6.54-11.9-4.5-2.04 2.03-.02 7.36 4.5 11.9 4.54 4.52 9.87 6.54 11.9 4.5Z"
    /><path
      d="M15.7 15.7c4.52-4.54 6.54-9.87 4.5-11.9-2.03-2.04-7.36-.02-11.9 4.5-4.52 4.54-6.54 9.87-4.5 11.9 2.03 2.04 7.36.02 11.9-4.5Z"
    />
  {:else if name === 'book'}
    <path d="M12 7v14" /><path
      d="M3 18a1 1 0 0 1-1-1V4a1 1 0 0 1 1-1h5a4 4 0 0 1 4 4 4 4 0 0 1 4-4h5a1 1 0 0 1 1 1v13a1 1 0 0 1-1 1h-6a3 3 0 0 0-3 3 3 3 0 0 0-3-3z"
    />
  {:else if name === 'badge'}
    <path
      d="M3.85 8.62a4 4 0 0 1 4.78-4.77 4 4 0 0 1 6.74 0 4 4 0 0 1 4.78 4.78 4 4 0 0 1 0 6.74 4 4 0 0 1-4.77 4.78 4 4 0 0 1-6.75 0 4 4 0 0 1-4.78-4.77 4 4 0 0 1 0-6.76Z"
    /><path d="m9 12 2 2 4-4" />
  {:else if name === 'users'}
    <path d="M16 21v-2a4 4 0 0 0-4-4H6a4 4 0 0 0-4 4v2" /><circle cx="9" cy="7" r="4" /><path
      d="M22 21v-2a4 4 0 0 0-3-3.87"
    /><path d="M16 3.13a4 4 0 0 1 0 7.75" />
  {:else if name === 'more'}
    <circle cx="12" cy="12" r="1" /><circle cx="19" cy="12" r="1" /><circle cx="5" cy="12" r="1" />
  {:else}
    <path d="M20 6 9 17l-5-5" />
  {/if}
</svg>
