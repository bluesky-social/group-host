// How the console's forms are sent: in place, with a script, so the page
// is drawn again where the reader is rather than loaded afresh at its top.
// Each form that posts takes one of these as `use:enhance={…}`; without a
// script, each still posts as a plain form does.

import type { enhance } from '$app/forms';
import { tick } from 'svelte';

/** What `use:enhance` is given: what a form does as it is sent. */
type SubmitFunction = NonNullable<Parameters<typeof enhance>[1]>;

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
 */
export const inPlace: SubmitFunction = () => {
  return async ({ result, update }) => {
    await update({ reset: false });
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
  return () => {
    failed(null);
    return async ({ result, update }) => {
      if (result.type === 'failure') {
        const why = (result.data as { failed?: string } | undefined)?.failed;
        failed(why ?? 'That didn’t work. Check what you entered.');
        return;
      }
      if (result.type === 'error') {
        failed(result.error?.message ?? 'Something went wrong on the host.');
        return;
      }
      // Closing it empties it (Dialog.svelte).
      document.getElementById(id)?.hidePopover();
      await update({ reset: false });
    };
  };
}
