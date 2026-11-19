// Which room is open, as the address says, and moving between rooms in
// place: `history.pushState`, which the kit shows in the address bar of
// the page around the site, so a reload or a copied link opens the room.
// The site is published with `index.html` as its fallback, so every
// room's address loads the app. A discussion opened is a room's own
// address under it: `discussion/<its author's DID>/<its key>`.

/** Each room, by its name, and what it is called. */
export const ROOMS = {
  board: 'Board',
  events: 'Events',
  discussion: 'Discussion',
  people: 'People'
};
const NAMED = Object.keys(ROOMS)
  .filter((r) => r !== 'board')
  .join('|');

/** Where the site's root is, as this frame was opened (`/_site/` in the host's page). */
const base = location.pathname.replace(new RegExp(`(${NAMED})(/.*)?$`), '');

/** Each room's address. */
export const hrefs = Object.fromEntries(
  Object.keys(ROOMS).map((r) => [r, r === 'board' ? base : `${base}${r}/`])
);

/** A discussion's address, by its author and key. */
export const topicHref = (did, rkey) => `${base}discussion/${did}/${rkey}`;

/** The room a path is, and the discussion it opens, if one: `{did, rkey}`. */
function at(path) {
  const [room, did, rkey] = path.slice(base.length).split('/');
  if (!(room in ROOMS) || room === 'board') return { room: 'board', topic: null };
  const topic = room === 'discussion' && did && rkey ? { did, rkey } : null;
  return { room, topic };
}

/** The room open now, and the discussion in it. */
export const route = $state(at(location.pathname));

const titled = (title) => (document.title = title ?? ROOMS[route.room]);
titled();

/**
 * Opens `href` in place, the room `to` and the discussion `topic` in it,
 * if one, for a click on a link to it; asked to open it elsewhere (a new
 * tab, say), leaves the click to the browser.
 */
export function go(e, to, topic = null) {
  if (e.metaKey || e.ctrlKey || e.shiftKey || e.altKey || e.button) return;
  e.preventDefault();
  if (to === route.room && topic?.rkey === route.topic?.rkey) return;
  route.room = to;
  route.topic = topic;
  titled();
  history.pushState(null, '', topic ? topicHref(topic.did, topic.rkey) : hrefs[to]);
}

/**
 * Names the tab for what is open, such as a discussion by its title, once
 * it is read: the address put back as it is, so the kit tells the page
 * around the site, which shows the tab's name.
 */
export const name = (title) => {
  titled(title);
  history.replaceState(history.state, '', location.href);
};

addEventListener('popstate', () => {
  Object.assign(route, at(location.pathname));
  titled();
});
