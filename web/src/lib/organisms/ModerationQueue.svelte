<script lang="ts">
	import Empty from '#lib/molecules/Empty.svelte';
	type Said = { $type?: string; reasonType?: string; reason?: string; status?: string; note?: string; val?: string; deleted?: boolean };
	type Event = { id: string; createdAt: string; createdBy?: string; event: Said };
	type Subject = { key: string; status: string; reportCount: number; events: Event[] };
	/**
	 * A group's moderation queue of one status: each subject, where it
	 * stands, what happened to it; for one who may, resolved or escalated,
	 * with a note.
	 */
	let { subjects, status, canResolve }: { subjects: Subject[]; status: string; canResolve: boolean } = $props();
	/** An event in a line: what it was, and what was said with it. */
	function said(e: Said): string {
		const kind = (e.$type ?? '').split('#')[1] ?? '';
		if (kind === 'reportEvent') return ['report', [e.reasonType?.split('#')[1], e.reason].filter(Boolean).join(': ')].filter(Boolean).join(': ');
		if (kind === 'statusEvent') return [e.status, e.note].filter(Boolean).join(': ');
		if (kind === 'labelEvent') return `${e.deleted ? 'label taken back' : 'label'}: ${e.val}`;
		if (kind === 'ejectEvent') return 'eject';
		return kind;
	}
</script>

{#if subjects.length}
	<ul class="queue">
		{#each subjects as s (s.key)}
			<li class="stack subject">
				<span class="mono">{s.key}</span>
				<span class="status">{s.status}</span>{#if s.reportCount}<span class="mute"> · reported {s.reportCount} times</span>{/if}
				<ol class="events">
					{#each s.events as e (e.id)}<li><span class="mute">{e.createdAt}</span> {said(e.event)}</li>{/each}
				</ol>
				{#if canResolve}
					<form method="post" action="?/resolve">
						<input type="hidden" name="subject" value={s.key} />
						<label>Note <input name="note" maxlength="2000" /></label>
						<button type="submit" name="status" value="resolved">Resolve</button>
						<button type="submit" name="status" value="escalated">Escalate</button>
					</form>
				{/if}
			</li>
		{/each}
	</ul>
{:else}
	<Empty title={`Nothing ${status}`}>Reports, labels and ejections show here while they are {status}.</Empty>
{/if}
