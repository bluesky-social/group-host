import { defineEnvVars } from '@sveltejs/kit/env';

export const variables = defineEnvVars({
  GROUP_HOST: {
    description: 'The group host, which answers the XRPC methods the pages read.',
    schema: (value) => (value || 'http://127.0.0.1:2600').replace(/\/$/, '')
  },
  FAVICONS: {
    description:
      "Where a link's site's icon is asked, as a PNG, when it is added: the address given ends where the link's address, encoded, follows. Empty asks nowhere.",
    schema: (value) => value ?? 'https://cardyb.bsky.app/v1/favicon?url='
  }
});
