<script lang="ts">
	import Dialog from '#lib/molecules/Dialog.svelte';
	import HandleInput from '#lib/molecules/HandleInput.svelte';
	/** Inviting someone to the group, by their handle, with a note if one likes: opened by a button whose `popovertarget` is `id`. */
	let { id, name }: { id: string; name: string } = $props();
</script>

<Dialog {id} title={`Invite someone to ${name}`}>
	<p class="mute">They'll see it the next time they sign in, here or in any app that shows invites.</p>
	<form class="stack" method="post" action="?/invite">
		<HandleInput id="{id}-handle" label="Their handle" placeholder="someone.bsky.social" />
		<label class="field">A note, if you like <textarea name="message" rows="3" maxlength="2000"></textarea></label>
		<div class="actions">
			<button class="quiet" type="button" popovertarget={id} popovertargetaction="hide">Cancel</button>
			<button type="submit">Send invite</button>
		</div>
	</form>
</Dialog>
