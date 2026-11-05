<script lang="ts">
	import FormDialog from '#lib/molecules/FormDialog.svelte';
	import HandleInput from '#lib/molecules/HandleInput.svelte';
	/** Inviting someone to the group, by their handle, with a note if one likes: opened by a button whose `popovertarget` is `id`. */
	let { id }: { id: string } = $props();
</script>

<FormDialog {id} title="Invite someone" action="?/invite" submit="Send invite">
	{#snippet intro()}They'll see it the next time they sign in, here or in any app that shows invites.{/snippet}
	<HandleInput id="{id}-handle" label="Their handle" placeholder="someone.bsky.social" />
	<label class="field">A note, if you like <textarea name="message" rows="3" maxlength="2000"></textarea></label>
</FormDialog>
