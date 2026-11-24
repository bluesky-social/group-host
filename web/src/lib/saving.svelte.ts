// A form that edits what is there in place (a group's profile, who sees
// it), saved by its one Save button: where it stands, said beside that
// button by `SaveBar`, and whether anything in it is not yet saved, which
// `LeaveGuard` asks about before the reader goes.

import type { enhance } from '$app/forms';
import { hold } from '#lib/forms.ts';
import { tick } from 'svelte';
import { m } from '#lib/paraglide/messages.js';
import { SvelteSet } from 'svelte/reactivity';

type SubmitFunction = NonNullable<Parameters<typeof enhance>[1]>;

/** Where a form stands: as it was, changed, being sent, saved, or refused. */
export type Standing = 'clean' | 'changed' | 'saving' | 'saved' | 'failed';

/** The forms on the page with changes not yet saved. */
const unsaved = new SvelteSet<Saving>();

/** Whether any form on the page has changes not yet saved. */
export const anyUnsaved = () => unsaved.size > 0;

/** What a form holds, as it would be sent. */
function held(form: HTMLFormElement) {
  return new URLSearchParams(new FormData(form) as unknown as Record<string, string>).toString();
}

export class Saving {
  standing = $state<Standing>('clean');
  /** Why it was refused, when it was. */
  why = $state<string | null>(null);
  #form: HTMLFormElement | null = null;
  #was = '';

  /** Whether it holds what is not saved yet. */
  get changed() {
    return unsaved.has(this);
  }

  /** What it holds now, against what it held when last drawn or saved. */
  #compare = async () => {
    await tick();
    if (!this.#form || this.standing === 'saving') return;
    const now = held(this.#form) !== this.#was;
    if (now) unsaved.add(this);
    else unsaved.delete(this);
    if (now && this.standing !== 'failed') this.standing = 'changed';
    if (!now && this.standing === 'changed') this.standing = 'clean';
  };

  /** `use:saving.track` on the form: what it holds now becomes what is saved. */
  track = (form: HTMLFormElement) => {
    this.#form = form;
    this.#was = held(form);
    for (const kind of ['input', 'change']) form.addEventListener(kind, this.#compare);
    return {
      destroy: () => {
        unsaved.delete(this);
        for (const kind of ['input', 'change']) form.removeEventListener(kind, this.#compare);
      }
    };
  };

  /**
   * `use:enhance={saving.submit}`: sent in place, as `inPlace` sends. Saved,
   * the page is drawn again with what was saved and the form says so;
   * refused, what was typed stays, and the form says why.
   */
  submit: SubmitFunction = ({ submitter }) => {
    this.standing = 'saving';
    this.why = null;
    const done = hold(submitter);
    return async ({ result, update }) => {
      done();
      if (result.type === 'failure' || result.type === 'error') {
        const why =
          result.type === 'failure'
            ? (result.data as { failed?: string } | undefined)?.failed
            : result.error?.message;
        this.why = why ?? m.saving_failed();
        this.standing = 'failed';
        return;
      }
      await update({ reset: false });
      await tick();
      if (this.#form) this.#was = held(this.#form);
      unsaved.delete(this);
      this.standing = 'saved';
    };
  };
}
