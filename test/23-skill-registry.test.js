/**
 * Tests for SkillRegistry — discovery, filtering, introspection, and health.
 */

import { describe, it } from 'node:test';
import assert from 'node:assert';
import { fileURLToPath } from 'url';
import { dirname, join } from 'path';
import { SkillRegistry, STABILITY_LEVELS } from '../lib/SkillRegistry.js';
import { SkillLoader } from '../lib/SkillLoader.js';

const __filename = fileURLToPath(import.meta.url);
const __dirname = dirname(__filename);
const SKILLS_DIR = join(__dirname, '..', 'skills');

describe('SkillRegistry', () => {
  const loader = new SkillLoader({ skillsDir: SKILLS_DIR });
  const registry = new SkillRegistry({ loader });

  it('discovers all skills', () => {
    const skills = registry.discover();
    assert.ok(skills.length >= 3, 'should discover at least 3 skills');
    const names = skills.map(s => s.name).sort();
    assert.ok(names.includes('universal_search'));
    assert.ok(names.includes('git_status'));
  });

  it('getAll returns discovered skills', () => {
    const skills = registry.getAll();
    assert.ok(Array.isArray(skills));
    assert.ok(skills.length >= 3);
  });

  it('filters by category', () => {
    const devSkills = registry.filter({ category: 'development' });
    assert.ok(devSkills.length >= 1, 'should have development skills');
    assert.ok(devSkills.every(s => s.category === 'development'));
  });

  it('filters by stability', () => {
    const stable = registry.filter({ stability: 'stable' });
    assert.ok(stable.length >= 1);
    assert.ok(stable.every(s => s.stability === 'stable'));
  });

  it('filters by minimum stability level', () => {
    const minBeta = registry.filter({ minStability: 'beta' });
    assert.ok(minBeta.length >= 1);
    assert.ok(minBeta.every(s => (STABILITY_LEVELS[s.stability] || 0) >= STABILITY_LEVELS.beta));
  });

  it('groups by category', () => {
    const map = registry.byCategory();
    assert.ok(map instanceof Map);
    assert.ok(map.has('development') || map.has('version_control') || map.has('search_navigation'));
    for (const [cat, skills] of map) {
      assert.ok(skills.every(s => (s.category || 'general') === cat));
    }
  });

  it('gets downstream chains for a skill', () => {
    const chains = registry.getDownstreamChains('universal_search');
    assert.ok(Array.isArray(chains));
    assert.ok(chains.length > 0, 'universal_search should have downstream chains');
    assert.ok(chains.every(c => c.skill && c.via));
  });

  it('gets upstream chains for a skill', () => {
    const upstream = registry.getUpstreamChains('git_diff');
    assert.ok(Array.isArray(upstream));
    // git_status lists git_diff as compatible_downstream
    const fromStatus = upstream.find(u => u.skill === 'git_status');
    assert.ok(fromStatus, 'git_status should chain into git_diff');
  });

  it('performs async health check', async () => {
    const health = await registry.checkHealth();
    assert.ok(Array.isArray(health));
    assert.ok(health.length >= 1);
    const entry = health.find(h => h.skill === 'git_status');
    assert.ok(entry);
    assert.ok(entry.primary);
    assert.strictEqual(typeof entry.primary.available, 'boolean');
  });

  it('summarizes health check', async () => {
    const summary = await registry.healthSummary();
    assert.strictEqual(typeof summary.total, 'number');
    assert.strictEqual(typeof summary.healthy, 'number');
    assert.strictEqual(typeof summary.unhealthy, 'number');
    assert.ok(Array.isArray(summary.missingPrimary));
    assert.ok(Array.isArray(summary.missingFallback));
    assert.ok(Array.isArray(summary.details));
  });

  it('finds skills by capability keyword', () => {
    const searchSkills = registry.findByCapability('search');
    assert.ok(searchSkills.length >= 1);
    const names = searchSkills.map(s => s.name);
    assert.ok(names.includes('universal_search'));
  });

  it('finds skills by command name', () => {
    const gitSkills = registry.findByCapability('git');
    assert.ok(gitSkills.length >= 1);
    assert.ok(gitSkills.some(s => s.name === 'git_status'));
  });

  it('returns install suggestions for missing dependencies', async () => {
    const suggestions = await registry.getInstallSuggestions();
    assert.ok(Array.isArray(suggestions));
    // We cannot assert specific missing deps because environment varies,
    // but we can assert shape consistency.
    for (const s of suggestions) {
      assert.ok(s.skill);
      assert.ok(s.dependency);
      assert.ok(s.type);
      assert.ok(s.suggestion);
    }
  });

  it('handles missing skill gracefully', () => {
    const downstream = registry.getDownstreamChains('nonexistent_skill_xyz');
    assert.deepStrictEqual(downstream, []);
    const upstream = registry.getUpstreamChains('nonexistent_skill_xyz');
    assert.deepStrictEqual(upstream, []);
  });
});

console.log('All SkillRegistry tests defined.');
