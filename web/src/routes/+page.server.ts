import type { Actions } from './$types';
import { act } from '#lib/data.ts';

/** An invitation, accepted or declined. */
export const actions: Actions = {
  accept: async (event) => {
    const group = String((await event.request.formData()).get('group'));
    return act(event, 'fyi.opensocial.requestJoin', { group });
  },
  decline: async (event) => {
    const group = String((await event.request.formData()).get('group'));
    return act(event, 'fyi.opensocial.internal.declineInvite', { group });
  }
};
