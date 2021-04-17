<!-- Copyright (c) 2021 Sony Pictures Imageworks, et al. -->
<!-- SPDX-License-Identifier: Apache-2.0 -->
<!-- https://github.com/imageworks/spk -->

# Open Source Execution Plan for SPK et al.

This is our working plan for how to turn this into a fully public,
community-developed and widely used project.

Contributors should propose edits to this, add milestones or tasks I have
forgotten, and check off items as they are completed. We'll only keep this
document alive as long as it is useful to us.

## Before any sharing

There are some housekeeping tasks to perform to the code base before anyone
outside Imageworks can have access to it:

- [x] Set up external (but private) repos, add internal developers.
- [x] Set up email list, add internal developers.
- [x] The code needs a LICENSE file attached, and copyright notices affixed
  to all the code modules.
- [ ] (IP - lg) README, CONTRIBUTING guide, basic procedures, etc. -- the set of
  things expected for any open source project.
- [ ] (IP - ryan) Some work to make it buildable and testable, so that the initial
  co-developers will actually be able to kick the tires.
- [ ] (IP - ryan) If possible, set up initial continuous integration so we know it
  builds outside of our environment and can pass basic tests.
- [ ] (IP - lg) Get **final** business clearance to open source.
- [ ] Add external developers to mail list, GitHub repos.


## Initial private share to work toward minimal viable product

The code is not currently ready to let just anybody use it -- it lacks a
simple way to build it outside our environment, needs documentation, comes
with no build/packaging recipes, etc. We don't want to go "public" until
it's in shape that it will make a good first impression and be useful for
people to start using.

We propose to host it on GitHub in the Imageworks area, but as private
repositories that are only visible with per-user permissions. We will
initially invite the other people who have expressed interest in helping us
to co-develop the software moving forward. As we start to talk more publicly
about what we're working on, we can also invite anyone else who subsequently
expresses an interest in helping with the process.

The purpose of this privately shared period is get to a "MVP" state, which
involves at least the following:

- [ ] Let the other presumed contributors see the code for the first time, try it out, evaluate it, figure out which parts need alteration to suit their needs, and which parts of the project moving forward they would like to help with.
- [ ] Set up a regular stakeholder meeting cadence.
- [ ] Combine the (currently) three repositories (for the file system layer, the packager, and the application launcher) into a single project/repository.
- [ ] Make sure that it's easily buildable and deployable outside the SPI environment, including build/deploy documentation.
- [ ] Set up a continuous integration system that can build and test the system, using GitHub Actions.
- [ ] Figure out how to organize package recipes within the repository, and seed it with package recipes for all the common VFX packages so that it will be a useful working system "out of the box" when it goes truly public.
- [ ] Set up web site and online documentation.
- [ ] Need logo and other basic graphic design assets.
- [ ] Achieve consensus on project governance -- who chairs meetings, how do we reach decisions, is there a chief architect with final call on technical disputes, how are people added to steering committee, etc.
- [ ] Anything else, technical or organizational, that comes up or is identified by the co-developers, that we think needs to be fixed before it's ready to be public.


## Public share

When the stakeholders think that the project is ready to launch publicly, we can just switch the existing repository from "private" to "public." We'll probably want some announcement or PR at that time.


## Possible ASWF future

It certainly seems like an ideal project to turn over to the ASWF. It is hard to determine the best timeline for this at this point, particularly before the other co-developers have been fully identified and before any of them have had a chance to see the project in action.

It may be that the right approach is to continue hosting it in the Imageworks GitHub account until it reaches a maturity level that the stakeholders decide they want to apply to turn it over to the ASWF. It is also possible that soon after the stakeholders see it and start working, they will desire to have it hosted in a truly neutral place like the ASWF as soon as possible, as a way to provide an even better foundation upon which many parties can collaborate. We will see how it goes and collectively decide when the time is right to try to have it hosted at ASWF.


## Proposed Timeline

This is all subject to change; this is just a stake in the ground to
document a possible future.

- Immediately: Set up private repos (without sharing with anyone outside), start working on the "pre-share" tasks listed above.
- By the end of next week (Apr 23), if possible: invite the outside stakeholders to have read permissions on the private repository.
- SIGGRAPH might be a good goal for a "go public" date and formal announcement about the project. The actual date for switching the repo to public visibility might be somewhat before or somewhat after, depending on how quickly we can accomplish the "private share" tasks.
- TBD when the right time is to apply for ASWF governance of the project, we'll have to see how the other steps go.
