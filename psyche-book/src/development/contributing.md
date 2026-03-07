## Contributing to Psyche

### **Found a bug?**

- **Make sure we're not already aware of it** by checking [GitHub Issues](https://github.com/PsycheFoundation/psyche/issues).

- If it seems your bug is new, [open an issue](https://github.com/PsycheFoundation/psyche/issues). Describe the expected & actual behaviour in as much detail as possible, ensuring to include system information (CUDA? CPU?) and any relevant command-line params (data parallelism? tensor parallelism? compression ratio?).

### **Fixed a bug?**

- Submit a GitHub PR with your bugfix.

- Make sure your PR clearly explains what was broken and how you fixed it. Reference any related issues.

- Before submitting, check out our [guidelines](#pr-guidelines) to keep things consistent.

### **Want to add a cool feature or change something?**

- First, share your idea on the [Psyche forum](https://forum.psyche.network/) and get some feedback.
- Feel free to start developing whenever you want, but we generally won't accept a PR unless there's been some discussion and feedback about whether your feature fits Psyche's goals.

### **Have questions about how things work?**

- Post your questions on the [Psyche forum](https://forum.psyche.network/) - that's the best place to get answers!

### **Want to improve our docs?**

- We'd love that. Feel free to open PRs!

Thank you for your contributions to Psyche :heart:

## PR guidelines

We prefer PRs to be made and merged using rebase, not merge commits.
It's not a deal-breaker, but rebase makes us happy \<3

### Clean Linear History

Rebasing creates a linear commit history without merges going back and forth, making it much easier to identify the place a change was made.
Fix-ups in merge commits that introduce bugs are no longer associated with the original code, whereas with rebase you'd find the bug as part of its original commit.

Merge commits add extra noise to the history without adding meaningful content about what changed.

### Better Bisect Experience

A linear history makes `git bisect` more effective for finding bugs, as each commit represents a coherent, working state of the codebase.

### Preserving Meaningful Commits

While we advocate for rebase, **we do not advocate for squashing all commits**. Each commit should:

1. **Document a single logical step** in your development process
2. **Be independently revertible** if needed
3. **Separate concerns** such as:
   - Refactoring (changing structure but not behavior)
   - Feature additions (changing behavior)
   - Bug fixes
   - Documentation updates
4. **Build & pass all checks** if checked out individually.

### What to Avoid

- **Don't squash meaningful commits together** - this buries important changes in large diffs and loses the step-by-step narrative
- **Don't use merge commits** within feature branches
- **Don't include "fix up" or "oops" commits** in your final PR - these are fine to have during development, but before opening your PR, use `git commit --amend` or interactive rebase to clean these up. A typical rebase workflow is explained [in this blog post](https://simondosda.github.io/posts/2022-01-03-git-rebase-workflow.html). [git absorb](https://andrewlock.net/super-charging-git-rebase-with-git-absorb/) is also very useful for small fix-ups.
