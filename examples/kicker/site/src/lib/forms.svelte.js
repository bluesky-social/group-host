// How a form is sent: held while what it sends is on its way, so it is
// never sent twice; emptied once done, if it adds a thing; and, refused,
// saying why beside it.

/**
 * A form's state, `busy` and `failed`, and `send`, its `onsubmit`: `act`
 * is given the form's data and does what it asks. A form that edits what
 * is there (`keep`) keeps its fields once sent.
 */
export function sending(act, { keep = false } = {}) {
  const state = $state({ busy: false, failed: '' });
  state.send = async (e) => {
    e.preventDefault();
    const form = e.currentTarget;
    state.busy = true;
    state.failed = '';
    try {
      await act(new FormData(form));
      if (!keep) form.reset();
    } catch (err) {
      state.failed = err.message;
    } finally {
      state.busy = false;
    }
  };
  return state;
}
