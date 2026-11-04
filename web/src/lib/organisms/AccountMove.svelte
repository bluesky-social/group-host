<script lang="ts">
	/**
	 * An account signed in to move here: before its move starts, what
	 * moving it would make of it, and its address here to choose; once
	 * started, the code its server emailed, to finish. `failed` is what
	 * the host said of the last try, if it refused it.
	 */
	type Moving = { did: string; handle?: string; pds?: string };
	type Plan = { handle: string; label: string; keeps: boolean; refused?: unknown };
	let { moving, plan, failed }: { moving: Moving; plan: Plan | null; failed?: string } = $props();
	const name = $derived(moving.handle ?? moving.did);
</script>

<div class="stack">
	{#if failed}<p class="danger">{failed}</p>{/if}
	{#if plan}
		<p>Moving <span class="mono">@{name}</span>, from <span class="mono">{moving.pds}</span>.</p>
		{#if plan.refused}
			<p class="danger">It can't have that address here: choose another.</p>
		{/if}
		<p>
			Here it will be <span class="mono">@{plan.handle}</span>{#if plan.keeps}, its own handle{/if}.
		</p>
		<form class="stack" method="get">
			<label class="field">
				Its address here
				<input name="address" value={plan.label} maxlength="63" pattern="[A-Za-z0-9\-]+" />
			</label>
			<button class="quiet" type="submit">Check that address</button>
		</form>
		<form class="stack" method="post" action="?/start">
			<input type="hidden" name="address" value={plan.label} />
			<p>Its posts, files and spaces are copied here, and its server emails you a code that lets it move.</p>
			<button type="submit" disabled={!!plan.refused}>Copy it here</button>
		</form>
	{:else}
		<p><span class="mono">@{name}</span> is copied here. Its server has emailed a code to the address the account has there.</p>
		<form class="stack" method="post" action="?/finish">
			<label class="field">The code <input name="code" required autocomplete="one-time-code" maxlength="64" /></label>
			<button type="submit">Finish moving it</button>
		</form>
	{/if}
	<form method="post" action="?/cancel">
		<button class="quiet" type="submit">Stop, and sign out of it</button>
	</form>
</div>
