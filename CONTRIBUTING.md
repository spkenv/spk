<!-- Copyright (c) 2021 Sony Pictures Imageworks, et al. -->
<!-- SPDX-License-Identifier: Apache-2.0 -->
<!-- https://github.com/imageworks/spk -->

# Contributing to SPK

Code contributions to SPK are always welcome. That's a big part of why it's
an open source project. Please review this document to get a briefing on our
process.


## Communications

* **Email**:

  [spk-dev](https://groups.google.com/g/spk-dev) is our email list. It is
  currently invitation only, but will be opened fully after the project is
  public.

* **Bug reports / Issue tracking**:

  Please use [GitHub Issues](https://github.com/imageworks/spk/issues).

  Eventually, the spk, spfs, and spawn repos will all be merged into a single
  respository. In the mean time, please use the spfs and spawn issues only
  for items specific to the code in those repos, and for any general topics,
  file issues in the spk repository so they will stick with what will
  eventually be the one true project.



## Licenses and copyright

SPK/SPFS/spawn are distributed using the [Apache-2.0 license](LICENSE.txt).

SPK/SPFS/spawn are Copyright (c) 2021 Sony Pictures Imageworks, et al.

Please note that the "et al" encompasses all the other contributors. We do
not require transfer of copyright -- technically speaking, submitters retain
copyright to all the individual changes or additions they make to the code,
and are merely licensing it to the other project users under the Apache 2.0
license.  "Sony" is named because the code was seeded from an internal
project and the repository still lives in the Imageworks GitHub account.
When the project is ready to be spun off into an independent organization
(such as ASWF), the copyright notices will all change to be more generic.

## DCO Sign-off

This project **does not** require a Contributor License Agreement (CLA)
for submitters. However, we do require a sign-off with the [Developer's Certificate of Origin 1.1
(DCO)](https://developercertificate.org/), which is the same mechanism that
the [Linux®
Kernel](https://git.kernel.org/pub/scm/linux/kernel/git/torvalds/linux.git/tree/Documentation/process/submitting-patches.rst#n416)
and many other communities use to manage code contributions. The DCO is
considered one of the simplest tools for sign offs from contributors as the
representations are meant to be easy to read and indicating signoff is done
as a part of the commit message.

Here is an example Signed-off-by line, which indicates that the submitter
accepts the DCO:

`Signed-off-by: John Doe <john.doe@example.com>`

You can include this automatically when you commit a change to your local
git repository using `git commit -s`. You might also want to leverage [this
command line tool](https://github.com/coderanger/dco) for automatically
adding the signoff message on commits.

> Rationale for DCO-only: (a) The ASWF and Linux Foundation and their legal
counsel believe that DCO provides enough protection to the project and its
users, and point to the fact that many critical industry projects (such as
the Linux kernel) find DCO sufficient. Out of all the open source
organizations under the Linux Foundation umbrella, ASWF is nearly alone in
using CLAs for most projects. Let's try the lighter weight approach this
time. (b) The nature of this project involves an expectation that many
individual contributors will submit "recipes" for building and packaging a
wide range of projects with spk, each recipe being a very small contribution
with no significant IP, and we wish to keep the contribution friction to an
absolute minimum. In past projects we have learned that requiring
contributors to get their employers' legal departments to evaluate and sign
CLAs very often makes developers decide that it's not worth bothering to
contribute at all (i.e., nobody wants to spend weeks fighting bureaucracy
just to submit a 6 line packaging recipe). (c) This also is less overhead
for Imageworks, which, if we use a full CLA, would be responsible for
collecting and recording the CLA signatures and making sure that all
contributors had them on file.


# Pull Requests and Code Review

The best way to submit changes is via GitHub Pull Request from your own
private fork of the repository. GitHub has a [Pull Request
Howto](https://help.github.com/articles/using-pull-requests/).

All code must be formally reviewed before being merged into the official
repository. The protocol is like this:

1. Get a GitHub account, fork imageworks/spk (or spfs or spawn) to create your
own repository on GitHub. During the time that we are keeping the imageworks
repos private (prior to announced public release of the open source project), please ensure that your own fork is also private.

2. Clone your repo to get a repository on your local machine, and you
   probably want to add a remote to the imageworks repo as well. From the
   command line, this probably looks like:

   ```
   $ git clone https://github.com/YOUR_GITHUB_USERID/spk.git
   $ cd spk
   $ git remote add imageworks https://github.com/imageworks/spk.git
   $ git fetch --all
   ```

3. Edit, compile, and test your changes in a topic branch:

   ```
   $ git checkout -b mytopic master
   $ ... do your edits ...
   ```

4. Push your changes to your fork (each unrelated pull request to a separate
"topic branch", please).

   ```
   $ git add ...files you changed...
   $ git commit -s -m "Your commit message"
   $ git push origin mytopic
   ```

5. Make a "pull request" on GitHub for your patch.

6. If your patch will induce a major compatibility break, or has a design
component that deserves extended discussion or debate among the wider spk
community, then it may be prudent to email spk-dev pointing everybody to
the pull request URL and discussing any issues you think are important.

7. The reviewer will look over the code and critique on the "comments" area,
or discuss in email. Reviewers may ask for changes, explain problems they
found, congratulate the author on a clever solution, etc. But until somebody
clicks the "approve" button in the review section, the code should not be
committed. Sometimes this takes a few rounds of give and take. Please don't
take it hard if your first try is not accepted. It happens to all of us.

8. After approval, one of the senior developers (with commit approval to the
official main repository) will merge your fixes into the master branch.

