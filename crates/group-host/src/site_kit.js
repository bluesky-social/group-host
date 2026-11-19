// `os`: what a page of a group's site asks the page the host serves
// around it, which holds the visitor's session. Each ask is a message to
// it, answered by one naming the same id; a read or a write nobody may
// make now is refused with `locked` and a reason, "sign in" or "not for
// you".
//
// <os-sign-in> is a button for signing in and out, in the page's own
// markup, so the page's own CSS styles it: "Sign in" while nobody is,
// then the visitor's handle and "Sign out".
(() => {
  let seq = 0;
  const pending = new Map();
  const watching = new Set();
  addEventListener('message', (e) => {
    const m = e.data;
    if (e.source !== parent || !m || m.os !== 1) return;
    if (m.type === 'changed') return watching.forEach((fn) => fn(m.me));
    if (m.type !== 'reply') return;
    const waiting = pending.get(m.id);
    if (!waiting) return;
    pending.delete(m.id);
    if (!('error' in m)) return waiting.resolve(m.value);
    const error = new Error(m.error);
    error.locked = !!m.locked;
    error.reason = m.reason;
    waiting.reject(error);
  });
  const ask = (type, body = {}) =>
    new Promise((resolve, reject) => {
      const id = ++seq;
      pending.set(id, { resolve, reject });
      parent.postMessage({ os: 1, type, id, ...body }, '*');
    });
  const tell = (type, body = {}) => parent.postMessage({ os: 1, type, ...body }, '*');
  const os = {
    /** The group whose site this is: `{did, handle}`. */
    group: () => ask('group'),
    /** Who is looking: `{signedIn, did, handle}`. */
    me: () => ask('me'),
    /**
     * One of the host's methods the site reads, as whoever is looking: its
     * JSON, or a `Blob` for one that answers a file (`getSpaceBlob`), to
     * show with `URL.createObjectURL`.
     */
    call: (method, params = {}) => ask('call', { method, params }),
    /**
     * Uploads `file`, a picture (a `File` or `Blob` of an `image/` type), to
     * the visitor's own PDS, for a record the site writes to point to.
     * Answers its reference, `{$type: 'blob', ref, mimeType, size}`, which
     * the record holds; a site that declares no writes may not upload.
     */
    upload: (file) => ask('upload', { file }),
    /**
     * Writes `record` in the visitor's own repo, in `space` (one of the
     * group's, `at://<group>/space/<type>/<skey>`), as the site declared it
     * may: in `collection`, at `rkey` if given. Answers its `uri` and `cid`.
     */
    create: (space, collection, record, { rkey } = {}) =>
      ask('create', { space, collection, record, ...(rkey ? { rkey } : {}) }),
    /** Writes `record` at `rkey`, in place of what is there, as `create` does. */
    put: (space, collection, rkey, record) => ask('put', { space, collection, rkey, record }),
    /** Deletes the visitor's own record at `rkey`, as `create` writes it. */
    delete: (space, collection, rkey) => ask('delete', { space, collection, rkey }),
    /**
     * Opens the host's sign-in dialog, its handle filled in with `hint` if
     * given, for the visitor to confirm. Signed in, the page loads again.
     */
    signIn: ({ hint } = {}) => tell('signIn', hint ? { hint: String(hint) } : {}),
    /** Signs the visitor out; `onChange` hears of it. */
    signOut: () => tell('signOut'),
    /** Calls `fn` with who is looking, whenever that changes in place. */
    onChange: (fn) => {
      watching.add(fn);
      return () => watching.delete(fn);
    }
  };
  window.os = os;

  // Where this page is and what it is called, for the page around it to
  // show in the address bar and the tab: as it loads, and as a page that
  // routes in the browser moves.
  const at = () =>
    tell('at', { path: location.pathname + location.search + location.hash, title: document.title });
  for (const name of ['pushState', 'replaceState']) {
    const was = history[name].bind(history);
    history[name] = (...args) => (was(...args), at());
  }
  addEventListener('popstate', at);
  addEventListener('hashchange', at);
  at();

  class SignIn extends HTMLElement {
    connectedCallback() {
      this.stop = os.onChange((me) => this.draw(me));
      os.me().then((me) => this.draw(me));
    }
    disconnectedCallback() {
      this.stop?.();
    }
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
