# Several projects: the fleet view

The fleet page shows every project's headline metrics side by side. Click a project to open its
dashboard.

1. Set up and collect each project in its own workspace folder ([getting-started.md](getting-started.md)).
2. Next to those folders, write a `fleet.toml` listing them, relative to the file
   ([example](../examples/fleet.toml)):

   ```toml
   name = "Acme Engineering"
   projects = ["acme-shop", "acme-crm", "acme-mobile"]
   ```

3. Run, from the folder with `fleet.toml`:

   ```sh
   pmx fleet serve --open            # local only: includes each project's people
   pmx fleet export                  # out/fleet.json + out/fleet.html, shareable, no per-person data
   pmx fleet export --with-private   # out/private/fleet.html with people, for the lead only
   ```

The fleet reads each project's `out/project.json`, so run `pmx collect` in each project first. Projects
collected on different dates are marked "as of <date>". Velocity is pooled only across projects scored
by the same classifier and rubric. How each fleet value is pooled:
[plan D34](../wiki/tool-implementation-plan.md).

Try it on fictional data: `pmx demo /tmp/acme-fleet --fleet && pmx -C /tmp/acme-fleet fleet serve --open`.
