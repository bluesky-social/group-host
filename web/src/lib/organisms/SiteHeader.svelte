<script lang="ts">
	import SignIn from '#lib/molecules/SignIn.svelte';
	import { page } from '$app/state';
</script>

<header class="top">
	<div class="wrap">
		<a class="brand" href="/">opensocial</a>
		<SignIn back={page.url.pathname} />
	</div>
</header>
