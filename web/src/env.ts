import { defineEnvVars } from '@sveltejs/kit/env';

export const variables = defineEnvVars({
	GROUP_HOST: {
		description: 'The group host, which answers the XRPC methods the pages read.',
		schema: (value) => (value || 'http://127.0.0.1:2600').replace(/\/$/, '')
	}
});
