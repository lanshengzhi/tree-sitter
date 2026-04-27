#!/usr/bin/env node
// orientation-handshake-harness.mjs
// End-to-end harness for R2 orientation handshake.
// Requires Node >= 18. Zero external dependencies.

import { strict as assert } from "node:assert";
import { spawnSync } from "node:child_process";
import { cp, mkdtemp, readFile, writeFile, rm } from "node:fs/promises";
import { join } from "node:path";
import { tmpdir } from "node:os";

const CARGO_RUN = [
  "cargo",
  "run",
  "--quiet",
  "-p",
  "tree-sitter-cli",
  "--bin",
  "tree-sitter-context",
  "--",
];

const FIXTURE_SRC = new URL(
  "../crates/cli/src/tests/fixtures/orientation_handshake",
  import.meta.url
).pathname;
const FIXTURE_STABLE_ID = "named:target";

// Project root for cargo run
const PROJECT_ROOT = new URL("..", import.meta.url).pathname;

function runCli(repoRoot, ...args) {
  const result = spawnSync(CARGO_RUN[0], [...CARGO_RUN.slice(1), ...args], {
    cwd: PROJECT_ROOT,
    encoding: "utf-8",
    timeout: 120_000,
    env: {
      ...process.env,
      // Ensure cargo uses the repo root even if spawned elsewhere
    },
  });
  return {
    status: result.status,
    stdout: result.stdout || "",
    stderr: result.stderr || "",
  };
}

async function main() {
  const tempDir = await mkdtemp(join(tmpdir(), "orientation-harness-"));
  try {
    // 1. Copy fixture to tempdir
    await cp(FIXTURE_SRC, tempDir, { recursive: true });

    // 2. Graph build
    const buildResult = runCli(tempDir, "graph", "build", "--repo-root", tempDir);
    assert.equal(
      buildResult.status,
      0,
      `[harness] step=graph_build reason=non_zero expected=0 actual=${buildResult.status} stderr=${buildResult.stderr}`
    );

    // 3. Orientation get
    const orientResult = runCli(
      tempDir,
      "orientation",
      "get",
      "--repo-root",
      tempDir,
      "--format",
      "json",
      "--budget",
      "2000"
    );
    assert.equal(
      orientResult.status,
      0,
      `[harness] step=orientation_get reason=non_zero expected=0 actual=${orientResult.status} stderr=${orientResult.stderr}`
    );

    let snapshotId;
    try {
      const parsed = JSON.parse(orientResult.stdout);
      snapshotId = parsed.graph_snapshot_id;
      assert.ok(
        snapshotId && snapshotId !== "no_graph" && snapshotId !== "unknown",
        `[harness] step=orientation_get reason=invalid_snapshot_id expected=valid_id actual=${snapshotId}`
      );
    } catch (e) {
      throw new Error(
        `[harness] step=orientation_get reason=parse_error expected=json actual=${orientResult.stdout} error=${e.message}`
      );
    }

    // 4. Bundle with snapshot ID -> fresh (sexpr format)
    const bundlePath = join(tempDir, "a.rs");
    const bundleResult = runCli(
      tempDir,
      "bundle",
      bundlePath,
      "--stable-id",
      FIXTURE_STABLE_ID,
      "--orientation-snapshot-id",
      snapshotId,
      "--format",
      "sexpr",
      "--max-tokens",
      "5000",
      "--budget",
      "500"
    );
    assert.equal(
      bundleResult.status,
      0,
      `[harness] step=bundle_fresh reason=non_zero expected=0 actual=${bundleResult.status} stderr=${bundleResult.stderr}`
    );

    const freshSexpr = bundleResult.stdout;
    const freshFreshnessMatch = freshSexpr.match(/\(orientation_freshness "([^"]+)"\)/);
    const freshSnapshotMatch = freshSexpr.match(/\(graph_snapshot_id "([^"]+)"\)/);
    assert.ok(
      freshFreshnessMatch,
      `[harness] step=bundle_fresh reason=missing_freshness expected=orientation_freshness actual=${freshSexpr}`
    );
    assert.equal(
      freshFreshnessMatch[1],
      "fresh",
      `[harness] step=bundle_fresh reason=freshness expected=fresh actual=${freshFreshnessMatch[1]}`
    );
    assert.ok(
      freshSnapshotMatch,
      `[harness] step=bundle_fresh reason=missing_snapshot_id expected=graph_snapshot_id actual=${freshSexpr}`
    );
    assert.equal(
      freshSnapshotMatch[1],
      snapshotId,
      `[harness] step=bundle_fresh reason=snapshot_id expected=${snapshotId} actual=${freshSnapshotMatch[1]}`
    );

    // 5. Graph postprocess (R3)
    const postprocessResult = runCli(tempDir, "graph", "postprocess", "--repo-root", tempDir);
    assert.equal(
      postprocessResult.status,
      0,
      `[harness] step=graph_postprocess reason=non_zero expected=0 actual=${postprocessResult.status} stderr=${postprocessResult.stderr}`
    );

    // 6. Orientation get after postprocess -> computed god_nodes (R3 assertion d)
    const orientPostprocessResult = runCli(
      tempDir,
      "orientation",
      "get",
      "--repo-root",
      tempDir,
      "--format",
      "sexpr",
      "--budget",
      "2000"
    );
    assert.equal(
      orientPostprocessResult.status,
      0,
      `[harness] step=orientation_postprocess reason=non_zero expected=0 actual=${orientPostprocessResult.status} stderr=${orientPostprocessResult.stderr}`
    );
    const postprocessSexpr = orientPostprocessResult.stdout;
    assert.ok(
      postprocessSexpr.includes('(god_nodes (computation_status "computed")'),
      `[harness] step=orientation_postprocess reason=missing_computed_god_nodes expected=computed_god_nodes actual=${postprocessSexpr}`
    );
    assert.ok(
      postprocessSexpr.includes('((rank 1)'),
      `[harness] step=orientation_postprocess reason=missing_rank_1 expected=rank_1_entry actual=${postprocessSexpr}`
    );

    // 7. Bundle with current snapshot ID -> fresh (R3 assertion e)
    const bundleFreshResult = runCli(
      tempDir,
      "bundle",
      bundlePath,
      "--stable-id",
      FIXTURE_STABLE_ID,
      "--orientation-snapshot-id",
      snapshotId,
      "--format",
      "sexpr",
      "--max-tokens",
      "5000",
      "--budget",
      "500"
    );
    assert.equal(
      bundleFreshResult.status,
      0,
      `[harness] step=bundle_postprocess_fresh reason=non_zero expected=0 actual=${bundleFreshResult.status} stderr=${bundleFreshResult.stderr}`
    );
    const freshPostprocessSexpr = bundleFreshResult.stdout;
    const freshPostprocessFreshnessMatch = freshPostprocessSexpr.match(/\(orientation_freshness "([^"]+)"\)/);
    const freshPostprocessSnapshotMatch = freshPostprocessSexpr.match(/\(graph_snapshot_id "([^"]+)"\)/);
    assert.ok(
      freshPostprocessFreshnessMatch,
      `[harness] step=bundle_postprocess_fresh reason=missing_freshness expected=orientation_freshness actual=${freshPostprocessSexpr}`
    );
    assert.equal(
      freshPostprocessFreshnessMatch[1],
      "fresh",
      `[harness] step=bundle_postprocess_fresh reason=freshness expected=fresh actual=${freshPostprocessFreshnessMatch[1]}`
    );
    assert.ok(
      freshPostprocessSnapshotMatch,
      `[harness] step=bundle_postprocess_fresh reason=missing_snapshot_id expected=graph_snapshot_id actual=${freshPostprocessSexpr}`
    );
    assert.equal(
      freshPostprocessSnapshotMatch[1],
      snapshotId,
      `[harness] step=bundle_postprocess_fresh reason=snapshot_id expected=${snapshotId} actual=${freshPostprocessSnapshotMatch[1]}`
    );

    // 8. Modify fixture again (change function body, not signature)
    const aRsPath = join(tempDir, "a.rs");
    const originalContent = await readFile(aRsPath, "utf-8");
    const modifiedContent = originalContent.replace(
      'println!("hello");',
      'println!("world");'
    );
    await writeFile(aRsPath, modifiedContent);

    // 9. Graph update (do NOT run postprocess)
    const updateResult = runCli(tempDir, "graph", "update", "--repo-root", tempDir);
    assert.equal(
      updateResult.status,
      0,
      `[harness] step=graph_update reason=non_zero expected=0 actual=${updateResult.status} stderr=${updateResult.stderr}`
    );

    // 10. Orientation get after update without re-postprocess -> unavailable (R3 assertion f)
    const orientStaleResult = runCli(
      tempDir,
      "orientation",
      "get",
      "--repo-root",
      tempDir,
      "--format",
      "sexpr",
      "--budget",
      "2000"
    );
    assert.equal(
      orientStaleResult.status,
      0,
      `[harness] step=orientation_stale reason=non_zero expected=0 actual=${orientStaleResult.status} stderr=${orientStaleResult.stderr}`
    );
    const staleOrientSexpr = orientStaleResult.stdout;
    assert.ok(
      staleOrientSexpr.includes('(god_nodes postprocess_unavailable)'),
      `[harness] step=orientation_stale reason=missing_postprocess_unexpected expected=postprocess_unavailable actual=${staleOrientSexpr}`
    );

    // 11. Bundle with old snapshot ID -> stale (sexpr format)
    const staleResult = runCli(
      tempDir,
      "bundle",
      bundlePath,
      "--stable-id",
      FIXTURE_STABLE_ID,
      "--orientation-snapshot-id",
      snapshotId,
      "--format",
      "sexpr",
      "--max-tokens",
      "5000",
      "--budget",
      "500"
    );
    assert.equal(
      staleResult.status,
      0,
      `[harness] step=bundle_stale reason=non_zero expected=0 actual=${staleResult.status} stderr=${staleResult.stderr}`
    );

    const staleSexpr = staleResult.stdout;
    const staleFreshnessMatch = staleSexpr.match(/\(orientation_freshness "([^"]+)"\)/);
    const staleSnapshotMatch = staleSexpr.match(/\(graph_snapshot_id "([^"]+)"\)/);
    assert.ok(
      staleFreshnessMatch,
      `[harness] step=bundle_stale reason=missing_freshness expected=orientation_freshness actual=${staleSexpr}`
    );
    assert.equal(
      staleFreshnessMatch[1],
      "stale",
      `[harness] step=bundle_stale reason=freshness expected=stale actual=${staleFreshnessMatch[1]}`
    );
    assert.ok(
      staleSnapshotMatch,
      `[harness] step=bundle_stale reason=missing_snapshot_id expected=graph_snapshot_id actual=${staleSexpr}`
    );
    assert.notEqual(
      staleSnapshotMatch[1],
      snapshotId,
      `[harness] step=bundle_stale reason=snapshot_id expected!=${snapshotId} actual=${staleSnapshotMatch[1]}`
    );

    console.log("[harness] all assertions passed");
    process.exit(0);
  } finally {
    await rm(tempDir, { recursive: true, force: true });
  }
}

main().catch((e) => {
  console.error(e.message);
  process.exit(1);
});
