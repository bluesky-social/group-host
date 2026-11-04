<script lang="ts">
	import Empty from '#lib/molecules/Empty.svelte';
	type Event = { kind: string; actor?: string; reason?: string; val?: string; note?: string; createdAt: string };
	type Subject = { subject: string; status: string; reportCount?: number; events: Event[] };
	/**
	 * A group's moderation queue: each subject, where it stands, what
	 * happened to it; for one who may, resolved, escalated, or noted on.
	 */
	let { subjects, canResolve }: { subjects: Subject[]; canResolve: boolean } = $props();
	const said = (e: Event) => [e.val, e.reason, e.note].filter(Boolean).join(': ');
</script>

{#if subjects.length}
	<ul class="queue">
		{#each subjects as s (s.subject)}
			<li class="stack subject">
				<span class="mono">{s.subject}</span>
				<span class="status">{s.status}</span>{#if s.reportCount}<span class="mute"> · reported {s.reportCount} times</span>{/if}
				<ol class="events">
					{#each s.events as e, i (i)}<li><span class="mute">{e.createdAt}</span> {e.kind}{said(e) ? `: ${said(e)}` : ''}</li>{/each}
				</ol>
				{#if canResolve}
					<form method="post" action="?/resolve">
						<input type="hidden" name="subject" value={s.subject} />
						<button type="submit" name="action" value="resolve">Resolve</button>
						<button type="submit" name="action" value="escalate">Escalate</button>
					</form>
					<form method="post" action="?/resolve">
						<input type="hidden" name="subject" value={s.subject} />
						<input type="hidden" name="action" value="note" />
						<label>Note <input name="note" required maxlength="1000" /></label>
						<button type="submit">Add the note</button>
					</form>
				{/if}
			</li>
		{/each}
	</ul>
{:else}
	<Empty title="Nothing to look at">Reports, labels and ejections show here.</Empty>
{/if}
