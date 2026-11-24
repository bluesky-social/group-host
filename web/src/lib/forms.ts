// How the console's forms are sent: in place, with a script, so the page
// is drawn again where the reader is rather than loaded afresh at its top.
// Each form that posts takes one of these as `use:enhance={…}`; without a
// script, each still posts as a plain form does.

import type { enhance } from '$app/forms';
import { tick } from 'svelte';
import { m } from '#lib/paraglide/messages.js';

/** What `use:enhance` is given: what a form does as it is sent. */
type SubmitFunction = NonNullable<Parameters<typeof enhance>[1]>;

/**
 * The button that sent a form, held down while it is on its way: it
 * cannot be pressed twice, and says it is busy. Returns what lets it go.
 */
export function hold(submitter: HTMLElement | null) {
  if (!(submitter instanceof HTMLButtonElement)) return () => {};
  submitter.disabled = true;
  submitter.setAttribute('aria-busy', 'true');
  return () => {
    submitter.disabled = false;
    submitter.removeAttribute('aria-busy');
  };
}

/** Scrolls to `el`, gently unless the reader asks for less motion. */
function show(el: Element | null) {
  const still = matchMedia('(prefers-reduced-motion: reduce)').matches;
  el?.scrollIntoView({ behavior: still ? 'auto' : 'smooth', block: 'center' });
}

/**
 * `form`'s fields that started empty, emptied again: what was typed into
 * a dialog that adds a thing is not there when it next opens, sent or
 * cancelled. A field
 * that shows what is there already (a role's name, its ticked actions) is
 * left to the page drawn again. Each is told, so a component holding its
 * value follows.
 */
export function empty(form: HTMLFormElement) {
  for (const field of form.querySelectorAll<HTMLInputElement | HTMLTextAreaElement>(
    'input, textarea'
  )) {
    if (
      ['hidden', 'checkbox', 'radio'].includes(field.type) ||
      field.defaultValue !== '' ||
      field.value === ''
    )
      continue;
    field.value = '';
    field.dispatchEvent(new Event('input', { bubbles: true }));
  }
}

/**
 * A form on the page, sent in place: what it changed drawn where the
 * reader is, what they typed kept. Refused, the page's notice says why,
 * scrolled to; sent elsewhere (a group just made), the page goes there.
 * Done, the page stays at its own address, query and all (a queue's
 * status): never the action's, which has none.
 */
export const inPlace: SubmitFunction = ({ submitter }) => {
  const done = hold(submitter);
  return async ({ result, update }) => {
    await update({ reset: false, navigate: false });
    done();
    if (result.type !== 'failure') return;
    await tick();
    show(document.querySelector('main p.danger'));
  };
};

/**
 * A form in the dialog `id`, sent in place: done, the dialog closes, the
 * page drawn again behind it.
 * Refused, it stays open with what was typed, and `failed` is told why,
 * to say in the dialog.
 */
export function inDialog(id: string, failed: (why: string | null) => void): SubmitFunction {
  return ({ submitter }) => {
    failed(null);
    const done = hold(submitter);
    return async ({ result, update }) => {
      done();
      if (result.type === 'failure') {
        const why = (result.data as { failed?: string } | undefined)?.failed;
        failed(why ?? m.forms_failed());
        return;
      }
      if (result.type === 'error') {
        failed(result.error?.message ?? m.forms_host_failed());
        return;
      }
      // Closing it empties it (Dialog.svelte).
      document.getElementById(id)?.hidePopover();
      await update({ reset: false, navigate: false });
    };
  };
}

/**
 * A form in a dialog that stays open once it is done (a label added to
 * what the dialog is about): the page drawn again behind it and in it.
 * Refused, `failed` is told why, to say in the dialog.
 */
export function inOpenDialog(failed: (why: string | null) => void): SubmitFunction {
  return ({ submitter }) => {
    failed(null);
    const done = hold(submitter);
    return async ({ result, update }) => {
      done();
      if (result.type === 'failure') {
        const why = (result.data as { failed?: string } | undefined)?.failed;
        failed(why ?? m.forms_failed());
        return;
      }
      if (result.type === 'error') {
        failed(result.error?.message ?? m.forms_host_failed());
        return;
      }
      await update({ reset: true, navigate: false });
    };
  };
}
