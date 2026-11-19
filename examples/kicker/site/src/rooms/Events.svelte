<script>
  import RoomHead from '../lib/molecules/RoomHead.svelte';
  import EventDialog from '../lib/organisms/EventDialog.svelte';
  import EventList from '../lib/organisms/EventList.svelte';
  import NewEventDialog from '../lib/organisms/NewEventDialog.svelte';
  import { follow, keep, kept } from '../lib/kept.svelte.js';
  import {
    NS,
    create,
    handleOf,
    isHost,
    keyOf,
    picture,
    put,
    remove,
    upload
  } from '../lib/place.js';
  import { ofCollection, recordAt, rsvps } from '../lib/views.js';
  /**
   * Events: the place's calendar, as cards, to come and past, each with
   * how many are going and the viewer's own answer, worked out from the
   * answers the room holds; and opened, in a dialog with who answered it.
   * Publishing one, for hosts and admins; answering one; and a host
   * calling their own off, or back on.
   */
  let { ctx } = $props();
  let failed = $state('');
  /** The event opened, by its URI, and whether its dialog is. */
  let opened = $state(null);
  let showing = $state(false);

  async function load() {
    try {
      await Promise.all([keep(ctx, 'calendar'), keep(ctx, 'room')]);
    } catch (e) {
      failed = e.message;
    }
  }
  $effect(() => {
    void ctx;
    load();
    const stops = [follow(ctx, 'calendar'), follow(ctx, 'room')];
    return () => stops.forEach((stop) => stop());
  });

  const then =
    (act) =>
    async (...args) => {
      await act(...args);
      await load();
    };

  /** The calendar's events and the answers to them, once both spaces are read. */
  const read = $derived(
    kept.calendar &&
      kept.room && {
        events: ofCollection(kept.calendar, `${NS}.event`),
        answers: rsvps(kept.room, `${NS}.rsvp`, ctx.me.did)
      }
  );
  const now = Date.now();
  const over = (e) => Date.parse(e.value.endsAt ?? e.value.startsAt) < now;
  const soonest = (a, b) => Date.parse(a.value.startsAt) - Date.parse(b.value.startsAt);

  /** An event as its card and dialog show it: who answered it, and what the viewer may do. */
  function row(event) {
    const said = read?.answers.to.get(event.uri) ?? { going: [], interested: [] };
    const handles = (status) => said[status].map((a) => handleOf(ctx, a.repo));
    const rkey = keyOf(event.uri);
    const image = event.value.image;
    return {
      event,
      host: handleOf(ctx, event.repo),
      picture: image ? picture(ctx, 'calendar', event.repo, image) : null,
      goingCount: said.going.length,
      going: handles('going'),
      interested: handles('interested'),
      past: over(event),
      mine: read?.answers.mine.get(event.uri) ?? 'none',
      answer: then((status) =>
        status === 'none'
          ? remove(ctx, 'room', 'rsvp', rkey)
          : put(
              ctx,
              'room',
              'rsvp',
              rkey,
              { subject: event.uri, status },
              recordAt(kept.room ?? [], ctx.me.did, `${NS}.rsvp`, rkey)
            )
      ),
      toggle:
        event.repo === ctx.me.did
          ? then(() =>
              put(
                ctx,
                'calendar',
                'event',
                rkey,
                { ...event.value, cancelled: !event.value.cancelled },
                event
              )
            )
          : null
    };
  }
  const all = $derived((read?.events ?? []).map(row));
  const coming = $derived(all.filter((r) => !r.past).sort((a, b) => soonest(a.event, b.event)));
  const past = $derived(all.filter((r) => r.past).sort((a, b) => soonest(b.event, a.event)));
  const open = (uri) => {
    opened = uri;
    showing = true;
  };
</script>

<RoomHead title="Events">
  {#snippet act()}{#if isHost(ctx)}<NewEventDialog
        publish={then(async (event, file) =>
          create(ctx, 'calendar', 'event', file ? { ...event, image: await upload(file) } : event)
        )}
      />{/if}{/snippet}
</RoomHead>
{#if failed}<p class="failed">{failed}</p>{/if}
{#if read}
  <EventList rows={coming} empty="Nothing planned yet" {open} />
  {#if past.length}
    <h2>Past events</h2>
    <EventList rows={past} empty="" past {open} />
  {/if}
{:else}
  <p class="meta">Reading the calendar…</p>
{/if}
<EventDialog bind:open={showing} row={all.find((r) => r.event.uri === opened)} />
