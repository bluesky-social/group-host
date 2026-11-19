// @ts-check
// `os`: what a page of a group's site asks the page the host serves
// around it (shell.ts), which holds the visitor's session. Each ask is a
// message to it, answered by one naming the same id (protocol.ts lists
// them); a read or a write nobody may make now is refused with `locked`
// and a reason, "sign in" or "not for you".
//
// A space of the group's is asked as @opensocial/sdk's GroupSpace is:
// `os.space(uri)` syncs it, holds its records here, and writes in it.
//
// <os-sign-in> is a button for signing in and out, in the page's own
// markup, so the page's own CSS styles it: "Sign in" while nobody is,
// then the visitor's handle and "Sign out".

/** @typedef {import('./protocol.ts').Asks} Asks */
/** @typedef {import('./protocol.ts').Me} Me */
/** @typedef {import('./protocol.ts').Reply | import('./protocol.ts').Push} FromHost */
/** @typedef {import('@opensocial/sdk').GroupRecord} GroupRecord */
/** @typedef {(records: GroupRecord[]) => void} Listener */

(() => {
  let seq = 0;
  /** @type {Map<number, {resolve: (value: any) => void, reject: (error: Error) => void}>} */
  const pending = new Map();
  /** @type {Set<(me: Me) => void>} */
  const watching = new Set();
  /**
   * The spaces this page opened, by URI, each with what holds the records
   * of a later read the host's page tells of.
   *
   * @type {Map<string, {space: ReturnType<typeof open>['space'], hold: Listener}>}
   */
  const spaces = new Map();

  addEventListener('message', (e) => {
    /** @type {FromHost} */
    const m = e.data;
    if (e.source !== parent || !m || m.os !== 1) return;
    if (m.type === 'changed') return watching.forEach((fn) => fn(m.me));
    if (m.type === 'records') return spaces.get(m.space)?.hold(m.records);
    if (m.type !== 'reply') return;
    const waiting = pending.get(m.id);
    if (!waiting) return;
    pending.delete(m.id);
    if (!('error' in m)) return waiting.resolve(m.value);
    const error = Object.assign(new Error(m.error), { locked: !!m.locked, reason: m.reason });
    if (m.name) error.name = m.name;
    waiting.reject(error);
  });

  /**
   * @template {keyof Asks} T
   * @param {T} type
   * @param {Asks[T]['ask']} body
   * @returns {Promise<Asks[T]['answer']>}
   */
  const ask = (type, body) =>
    new Promise((resolve, reject) => {
      const id = ++seq;
      pending.set(id, { resolve, reject });
      parent.postMessage({ os: 1, type, id, ...body }, '*');
    });
  /** @param {string} type @param {object} [body] */
  const tell = (type, body = {}) => parent.postMessage({ os: 1, type, ...body }, '*');

  /**
   * One of the group's spaces, `at://<group>/space/<type>/<skey>`, as the
   * SDK's GroupSpace: its records synced and held here, and the visitor's
   * own records in it written.
   *
   * @param {string} uri
   */
  function open(uri) {
    /** @type {GroupRecord[] | undefined} */
    let held;
    /** @type {Set<Listener>} */
    const listeners = new Set();
    /** @type {Listener} */
    const hold = (records) => {
      held = records;
      listeners.forEach((fn) => fn(records));
    };
    const space = {
      /** The space's URI. */
      uri,
      /**
       * Brings the records held up to date, read as the visitor from each
       * writer's repo in their browser, and checked: the first time every
       * writer's, after only what changed. Answers `{failed}`, the writers
       * whose host failed, whose last records are kept.
       */
      sync: async () => {
        const { failed, records } = await ask('sync', { space: uri });
        // Changed since this page last held them: said by the host's page
        // before this answer, unless this page held none yet.
        if (held === undefined) hold(records);
        else held = records;
        return { failed };
      },
      /**
       * The records held now, each `{uri, repo, collection, rkey, cid,
       * value}`, of every writer, or only of `collections` (NSIDs).
       *
       * @param {{collections?: readonly (string | {$type: string})[]}} [options]
       */
      records: ({ collections } = {}) => {
        if (!collections) return [...(held ?? [])];
        const wanted = new Set(collections.map((c) => (typeof c === 'string' ? c : c.$type)));
        return (held ?? []).filter((r) => wanted.has(r.collection));
      },
      /**
       * Calls `fn` with every record held after each sync that changed
       * them. Returns a function that stops it.
       *
       * @param {Listener} fn
       */
      onChange: (fn) => {
        listeners.add(fn);
        return () => {
          listeners.delete(fn);
        };
      },
      /**
       * Creates `value`, a record of `collection` (its `$type`), in the
       * visitor's own repo in the space, as the site declared it may: at
       * `rkey` if given. Answers its `uri` and `cid`.
       *
       * @param {string} collection
       * @param {object} value
       * @param {{rkey?: string}} [options]
       */
      create: (collection, value, { rkey } = {}) =>
        ask('create', { space: uri, collection, value, ...(rkey ? { rkey } : {}) }),
      /**
       * Creates or replaces the visitor's record at `rkey`, as `create`.
       *
       * @param {string} collection
       * @param {object} value
       * @param {{rkey: string}} options
       */
      put: (collection, value, { rkey }) => ask('put', { space: uri, collection, value, rkey }),
      /**
       * Deletes the visitor's own record at `rkey`.
       *
       * @param {string} collection
       * @param {{rkey: string}} options
       */
      delete: async (collection, { rkey }) => {
        await ask('delete', { space: uri, collection, rkey });
      }
    };
    return { space, hold };
  }

  const os = {
    /**
     * The group whose site this is: `{did, handle, console}`, `console`
     * the address of the host's console, where its members manage it.
     */
    group: () => ask('group', {}),
    /** Who is looking: `{signedIn, did, handle}`. */
    me: () => ask('me', {}),
    /**
     * One of the host's methods the site reads, as whoever is looking: its
     * JSON, or a `Blob` for one that answers a file (`getSpaceBlob`), to
     * show with `URL.createObjectURL`.
     *
     * @param {string} method
     * @param {Record<string, unknown>} [params]
     */
    call: (method, params = {}) => ask('call', { method, params }),
    /**
     * One of the host's procedures the site acts by, as whoever is looking:
     * `input` its JSON, answered with what the method answers, its `group`
     * always this site's. Such as `fyi.opensocial.createReport`, reporting
     * what they see, or `fyi.opensocial.requestJoin`, asking to join.
     *
     * @param {string} method
     * @param {object} [input]
     */
    act: (method, input = {}) => ask('act', { method, input }),
    /**
     * One of the group's spaces, `at://<group>/space/<type>/<skey>`, of a
     * type the site declares in `spaces` to read it, or in `writes` to
     * write in it: the same each time it is asked for on this page.
     *
     * @param {string} uri
     */
    space: (uri) => {
      const found = spaces.get(uri) ?? open(uri);
      spaces.set(uri, found);
      return found.space;
    },
    /**
     * Uploads `file`, a picture (a `File` or `Blob` of an `image/` type), to
     * the visitor's own PDS, for a record the site writes to point to.
     * Answers its reference, `{$type: 'blob', ref, mimeType, size}`, which
     * the record holds; a site that declares no writes may not upload.
     *
     * @param {Blob} file
     */
    upload: (file) => ask('upload', { file }),
    /**
     * Opens the host's sign-in dialog, its handle filled in with `hint` if
     * given, for the visitor to confirm. Signed in, the page loads again.
     *
     * @param {{hint?: string}} [options]
     */
    signIn: ({ hint } = {}) => tell('signIn', hint ? { hint: String(hint) } : {}),
    /** Signs the visitor out; `onChange` hears of it. */
    signOut: () => tell('signOut'),
    /**
     * Calls `fn` with who is looking, whenever that changes in place.
     *
     * @param {(me: Me) => void} fn
     */
    onChange: (fn) => {
      watching.add(fn);
      return () => {
        watching.delete(fn);
      };
    }
  };
  Object.assign(window, { os });

  // Where this page is and what it is called, for the page around it to
  // show in the address bar and the tab: as it loads, and as a page that
  // routes in the browser moves.
  const at = () =>
    tell('at', {
      path: location.pathname + location.search + location.hash,
      title: document.title
    });
  for (const name of /** @type {const} */ (['pushState', 'replaceState'])) {
    const was = history[name].bind(history);
    history[name] = (...args) => (was(...args), at());
  }
  addEventListener('popstate', at);
  addEventListener('hashchange', at);
  at();

  class SignIn extends HTMLElement {
    /** @type {(() => void) | undefined} */
    stop;
    connectedCallback() {
      this.stop = os.onChange((me) => this.draw(me));
      os.me().then((me) => this.draw(me));
    }
    disconnectedCallback() {
      this.stop?.();
    }
    /** @param {Me} me */
    draw(me) {
      const button = document.createElement('button');
      button.type = 'button';
      if (!me.signedIn) {
        button.textContent = 'Sign in';
        button.onclick = () => os.signIn({ hint: this.getAttribute('hint') ?? undefined });
        return this.replaceChildren(button);
      }
      const who = document.createElement('span');
      who.className = 'handle';
      who.textContent = `@${me.handle ?? me.did}`;
      button.textContent = 'Sign out';
      button.onclick = () => os.signOut();
      this.replaceChildren(who, ' ', button);
    }
  }
  customElements.define('os-sign-in', SignIn);
})();
