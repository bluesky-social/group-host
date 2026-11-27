# Signing in as a group in another app

This guide shows you how to let another app post as your group, such as
a bot or a script. A group has no password of its own, so you create an
app password for the app. An app password works for one app, and you
can revoke it at any time without changing anything else.

Some apps sign in as a group without a password, as they sign you in.
The app sends you to the console, and you approve it there, as
[How do I approve an app that asks to sign in as the group?](#how-do-i-approve-an-app-that-asks-to-sign-in-as-the-group)
shows.

## Before you begin

You need a group where you have the admin role. If you have none yet,
[start one](starting-a-group.md) first.

## Where do I find the group's apps?

1. Open your group's page. Here it is "Night shift", and you have the
   admin role.

   <!-- look /new --as bob --type '[name=displayName]=Night shift' --type '[name=label]=nightshift' --click 'button:text-is("Start the group")' -->
   ![Night shift's page, with Moderation and Settings at the top right, and "You're a member with the admin role."](shots/apps-new.png)

2. Press **Settings**, then the **Apps** tab. It has three cards, empty
   for now: **Apps acting as the group**, **App passwords** and
   **Changes made as the group**.

   <!-- look /g/nightshift.groups.test/settings/apps --as bob -->
   ![The Apps tab of Night shift's settings: Apps acting as the group 0, App passwords 0 with Create an app password, and Changes made as the group, each empty](shots/apps-empty.png)

By default only admins may create app passwords. A member may create
them too if one of their roles may write every collection as the group.
That permission isn't among those in
[Rules and roles](rules-and-roles.md), since the console can't give it.
You give it from a terminal, with `--permissions 'repo:*'` on
`atgrp role set` or `atgrp role grant`. In **Settings**, such
a member sees only the **App passwords** card, with the passwords they
made.

## How do I create an app password?

1. Under **App passwords**, press **Create an app password**. A dialog
   opens.

   <!-- look /g/nightshift.groups.test/settings/apps --as bob --click 'button[popovertarget=make-password]' --element ':popover-open' -->
   ![The Create an app password dialog: You'll see it once, right after it's created. A field Where it is used, with My phone in grey, a checkbox It may use chat, and Cancel and Create](shots/apps-dialog.png)

2. In **Where it is used**, type a name for the app, such as "Phone". The
   name is only for you, so you can tell your passwords apart.
3. **It may use chat** lets the app use Bluesky's chat as the group.
   This host answers no chat yet, so leave it unticked unless the app
   needs chat.
4. Press **Create**.
5. The password shows once, in a notice above the list. There's no Copy
   button, so select the password and copy it. Keep it somewhere safe.
   The host doesn't keep it, and you won't see it again. The password
   in the picture is an example, and yours will differ.

   <!-- look /g/nightshift.groups.test/settings/apps --as bob --click 'button[popovertarget=make-password]' --type '[name=name]=Phone' --click ':popover-open button:text-is("Create")' --element 'section:has(h2:text-is("App passwords"))' -->
   ![App passwords 1, with a notice: The app password "Phone" is followed by the password, Copy it now. You won't see it again. Below, a row Phone, By bob.test, with Revoke](shots/apps-made.png)

Anyone with the password can post as the group. Give it only to an app
you trust. If it leaks, revoke it at once, as
[How do I stop an app?](#how-do-i-stop-an-app) shows.

The password stays yours. It doesn't expire, but it stops working if you
lose the role that let you make it, or leave the group.

## What do I type into the other app?

In the other app's sign-in, type these three things:

- Server: the address of the console you're using, the one in your
  browser's address bar. The console passes sign-ins on to the host. In
  the Bluesky app this is "Hosting provider", which you change from
  Bluesky Social to a custom one.
- Username: the group's handle, such as "nightshift.groups.test".
- Password: the app password you just copied.

This isn't your own handle or your own password. An app password signs
an app in only as the group. The host turns down a wrong password, or a
handle that isn't a group on this host.

## What can the app do as the group?

The app can write the group's own records: post as the group, delete its
posts, upload pictures, like and follow, and change the group's profile.
It can't write to anyone else's account, and it can't see or change
what the group keeps only for its members.

The host doesn't serve feeds. If you sign the Bluesky app in as the
group, its timeline will look empty or broken. Posting still works. So
the app is for posting as the group, as a bot or a script does.

## How do I approve an app that asks to sign in as the group?

Some apps sign in as a group the way they sign you in, with no
password. You start in the app. It asks for the group's handle and
sends you to the console.

1. The console shows the app's name and icon, and what it would get as
   the group. Here the app is "Darkroom", asking to post as Night shift
   and to keep its own profile for the group.

   <!-- look apps:/darkroom/sign-in?hint=nightshift.groups.test --as bob -->
   ![Sign in an app: Darkroom's icon, and Darkroom wants to sign in as one of your groups. Below, Night shift, what it would get, Bluesky and Repository, then Sign it in as Night shift and Decline](shots/apps-approve.png)

2. Press **Sign it in as Night shift**. You're sent back to the app,
   which now acts as the group. Press **Decline** to send it back with
   nothing.

The app gets only what your roles let you give. What it asks for beyond
that is listed apart, and it doesn't get it.

## Where do I see what the app does?

1. Open **Settings** and **Apps** again. Under
   **Apps acting as the group**, each app signed in as the group has a
   row. An app signed in with a password is named for the password. An
   app you approved shows its own name and icon, and the domain it
   signed in from. Each row says what the app can do, who approved it
   or made its password, and when it was last used.

   <!-- look /g/nightshift.groups.test/settings/apps --as bob --seed nightshift-oauth --seed nightshift-apps --element 'section:has(h2:text-is("Apps acting as the group"))' -->
   ![Apps acting as the group 3: Phone, Can post and make changes as the group, With an app password created by bob.test; Darkroom, Can read the group's profile and members, and see requests to join, Approved by bob.test; Darkroom with its icon, Can post and make changes as the group, Approved by bob.test; each with Revoke](shots/apps-signed-in.png)

   One app can sign in more than once, for different things. Here
   Darkroom posts as the group, and also reads its members to show them
   in Darkroom. The line under each name tells them apart.

2. Under **Changes made as the group**, each change made as the group
   is listed, newest first. That's what apps wrote, with who approved
   each, and what members changed here in the console, such as who sees
   the group.

   <!-- look /g/nightshift.groups.test/settings/apps --as bob --element 'section:has(h2:text-is("Changes made as the group"))' -->
   ![Changes made as the group: Phone deleted a post, Phone posted, Phone posted, each App password created by bob.test; bob.test made the group members-only, In the console; Darkroom created the group's Darkroom profile, Approved by bob.test](shots/apps-activity.png)

The list shows only changes, and no reads. It stays after you revoke an
app or its password, so you can still see what it did.

## How do I stop an app?

You can end one app's sign-in, or revoke the password itself.

1. To sign an app out, press **Revoke** on its row under
   **Apps acting as the group**, then **Revoke** in the dialog.

   <!-- look /g/nightshift.groups.test/settings/apps --as bob --click 'button[popovertarget=revoke-0]' --element ':popover-open' -->
   ![Revoke this app's access? Phone can no longer act as the group. It can sign in again later. Cancel and Revoke](shots/apps-revoke-access.png)

   The app is signed out at once. The password still works, so an app
   that kept it, as a script does, can sign in again.

2. To stop the password for good, press **Revoke** on its row under
   **App passwords**, then **Revoke** in the dialog.

   <!-- look /g/nightshift.groups.test/settings/apps --as bob --click 'button[popovertarget=unpassword-0]' --element ':popover-open' -->
   ![Revoke this app password? Apps using "Phone" are signed out, and it stops working. Cancel and Revoke](shots/apps-revoke-password.png)

   Every app signed in with it is signed out, and the password no longer
   signs anyone in.

You may revoke the passwords you made. An admin may revoke anyone's.

## From a terminal

There's no command for app passwords or for the apps acting as the
group. You make and revoke them in the console. The
[CLI reference](../reference/cli.md) lists the commands there are.

Next, you may want to give members roles in [Rules and roles](rules-and-roles.md),
keep the group's posts in order with [Moderating](moderating.md), or set
up [Recovery keys](recovery-keys.md). [Signing in](../design/signing-in.md)
explains how signing in works.

Connecting an AI app isn't the same as signing in as the group. An app
added under **Connect an AI app** on the group's page acts for you, with
what your roles let you do, and needs no app password, as
[AI apps](../design/mcp.md) explains.
