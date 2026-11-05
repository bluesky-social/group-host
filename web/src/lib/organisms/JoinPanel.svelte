<script lang="ts">
	/**
	 * How the person looking gets into the group, as its join policy says
	 * and as they stand: in at once by an invite, whatever the policy,
	 * or where anyone can join, an invite declined as it is on the home
	 * page, asked nothing as another can be sent; asking, with a note, where someone looks at
	 * each request, and then waiting, which they may cancel; or told to
	 * ask for an invite. Once `joined`, a welcome.
	 */
	let {
		name,
		joinPolicy,
		asked = false,
		invited = false,
		joined = false
	}: { name: string; joinPolicy?: string; asked?: boolean; invited?: boolean; joined?: boolean } = $props();
	const policy = $derived(joinPolicy ?? 'approval');
</script>

<section class="card stack join">
	{#if joined}
		<h2>You're in</h2>
		<p class="mute">Welcome to {name}.</p>
	{:else if invited}
		<h2>You're invited</h2>
		<p class="mute">Someone in {name} invited you to join.</p>
		<div class="choices">
			<form method="post" action="?/decline"><button class="quiet" type="submit">Decline</button></form>
			<form method="post" action="?/join"><button type="submit">Join</button></form>
		</div>
	{:else if asked}
		<h2>Your request is waiting</h2>
		<p class="mute">You'll be a member once someone approves it.</p>
		<form method="post" action="?/cancel"><button class="quiet" type="submit">Cancel request</button></form>
	{:else if policy === 'open'}
		<h2>Join {name}</h2>
		<p class="mute">Anyone can join.</p>
		<form method="post" action="?/join"><button type="submit">Join</button></form>
	{:else if policy === 'invite'}
		<h2>Joining is by invite</h2>
		<p class="mute">Ask someone in {name} to invite you. Your invite will show up here.</p>
	{:else}
		<h2>Ask to join</h2>
		<p class="mute">Someone in {name} reviews each request.</p>
		<form class="stack" method="post" action="?/join">
			<label class="field">A note to them, if you like <textarea name="message" rows="3" maxlength="2000"></textarea></label>
			<button type="submit">Send request</button>
		</form>
	{/if}
</section>
