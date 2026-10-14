Focus: the instructions that steer agents (CLAUDE.md files, skills, product context)

Read first: the header of the root CLAUDE.md (each rule is stated once, in the section
that owns it).

Check:
- A rule that contradicts another CLAUDE.md, a skill, or the code as it now is. Check the
  code: a rule naming a function, flag, file or command that does not exist is wrong.
- A rule stated a second time somewhere else.
- Content added to an always-loaded file that anyone could get from the code in a
  minute. It costs every call; it belongs in the code or a crate-local CLAUDE.md.
- An instruction that would have an agent do something unsafe or outward-facing without
  confirmation: push to main, force-push, touch production, apply `review-override`.
- A skill whose description would not trigger when it should, or would trigger on
  unrelated work.
