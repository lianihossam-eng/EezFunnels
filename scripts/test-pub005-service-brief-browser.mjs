import { chromium, expect } from '@playwright/test';
import axeCore from 'axe-core';
import http from 'node:http';

const baseUrl = process.env.PUB005_BASE_URL;
const funnelId = process.env.PUB005_FUNNEL_ID;
if (!baseUrl || !funnelId) throw new Error('PUB005_BASE_URL and PUB005_FUNNEL_ID are required');

const browser = await chromium.launch({ headless: true });
try {
  const page = await browser.newPage({ viewport: { width: 1280, height: 900 } });
  await page.goto(`${baseUrl}/funnels/${funnelId}/brief`, { waitUntil: 'networkidle' });
  await expect(page.getByRole('heading', { name: 'Service brief', level: 1 })).toBeVisible();
  await expect(page.getByRole('heading', { name: 'Service context' })).toBeVisible();

  const radioOptions = page.locator('input[name="service-intention"]');
  await radioOptions.first().focus();
  await radioOptions.first().press('ArrowDown');
  await expect(radioOptions.nth(1)).toBeChecked();
  await page.locator('#service-brief-audience').fill('PRIVATE_PUB005_SENTINEL local studios');
  await page.getByRole('button', { name: 'Save private brief' }).click();
  await expect(page.getByText('Service brief saved as version 1.')).toBeVisible();

  await page.reload({ waitUntil: 'networkidle' });
  await expect(page.locator('#service-brief-audience')).toHaveValue(
    'PRIVATE_PUB005_SENTINEL local studios',
  );
  await page.locator('#service-brief-audience').fill('PRIVATE_PUB005_SENTINEL local studios, updated locally');

  const externalWrite = await page.evaluate(async ({ funnelId }) => {
    const response = await fetch(`/api/funnels/${funnelId}/service-brief`, {
      method: 'PUT',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({
        expected_version: 1,
        format_version: 1,
        brief: { intention: 'contact', offer: 'A newer private offer' },
      }),
    });
    return { status: response.status, body: await response.json() };
  }, { funnelId });
  expect(externalWrite.status).toBe(200);

  await page.getByRole('button', { name: 'Save private brief' }).click();
  await expect(page.getByText('Latest saved version: 2.')).toBeVisible();
  await expect(page.getByRole('heading', { name: 'Latest saved brief (version 2)' })).toBeVisible();
  await expect(page.locator('.service-brief-conflict-summary')).toContainText('A newer private offer');
  await expect(page.locator('#service-brief-audience')).toHaveValue(
    'PRIVATE_PUB005_SENTINEL local studios, updated locally',
  );
  await page.getByRole('button', { name: 'Save my draft over latest' }).click();
  await expect(page.getByText('Service brief saved as version 3.')).toBeVisible();

  const saved = await page.evaluate(async ({ funnelId }) => {
    const response = await fetch(`/api/funnels/${funnelId}/service-brief`);
    return { status: response.status, cache: response.headers.get('cache-control'), body: await response.json() };
  }, { funnelId });
  expect(saved.status).toBe(200);
  expect(saved.cache).toContain('private');
  expect(saved.cache).toContain('no-store');
  expect(saved.body.version).toBe(3);
  expect(saved.body.brief.audience).toBe('PRIVATE_PUB005_SENTINEL local studios, updated locally');

  const publicSurfaces = await page.evaluate(async ({ funnelId }) => {
    const paths = [
      '/api/funnels',
      `/api/funnels/${funnelId}/completions?period=7`,
      `/api/funnels/${funnelId}/interaction-report?period=7`,
      `/api/submissions?funnel_id=${funnelId}`,
    ];
    const responses = await Promise.all(paths.map(async path => {
      const response = await fetch(path);
      return { status: response.status, body: await response.text() };
    }));
    return responses;
  }, { funnelId });
  for (const response of publicSurfaces) {
    expect(response.status).toBe(200);
    expect(response.body).not.toContain('PRIVATE_PUB005_SENTINEL');
  }

  const quoteRun = '"'.repeat(4096);
  const escapedWrite = await page.evaluate(async ({ funnelId, quoteRun }) => {
    const response = await fetch(`/api/funnels/${funnelId}/service-brief`, {
      method: 'PUT',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({
        expected_version: 3,
        format_version: 1,
        brief: { intention: 'contact', audience: quoteRun, need: quoteRun, offer: quoteRun, scope: quoteRun },
      }),
    });
    return { status: response.status, cache: response.headers.get('cache-control'), body: await response.json() };
  }, { funnelId, quoteRun });
  expect(escapedWrite.status).toBe(200);
  expect(escapedWrite.body.version).toBe(4);

  const exactRetry = await page.evaluate(async ({ funnelId, quoteRun }) => {
    const response = await fetch(`/api/funnels/${funnelId}/service-brief`, {
      method: 'PUT',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({
        expected_version: 3,
        format_version: 1,
        brief: { intention: 'contact', audience: quoteRun, need: quoteRun, offer: quoteRun, scope: quoteRun },
      }),
    });
    return { status: response.status, body: await response.json() };
  }, { funnelId, quoteRun });
  expect(exactRetry.status).toBe(200);
  expect(exactRetry.body.version).toBe(4);

  const noStorePaths = [`/api/funnels/${funnelId}/service-brief`, '/api/funnels/22222222-2222-4222-8222-222222222222/service-brief'];
  for (const path of noStorePaths) {
    const response = await fetch(new URL(path, baseUrl));
    expect(response.headers.get('cache-control')).toContain('private');
    expect(response.headers.get('cache-control')).toContain('no-store');
  }

  const errorRequests = [
    fetch(new URL(`/api/funnels/${funnelId}/service-brief`, baseUrl), {
      method: 'PUT',
      headers: { Origin: 'https://attacker.example', 'Content-Type': 'application/json' },
      body: JSON.stringify({ expected_version: 4, format_version: 1, brief: {} }),
    }),
    fetch(new URL(`/api/funnels/${funnelId}/service-brief`, baseUrl), {
      method: 'PUT',
      body: JSON.stringify({ expected_version: 4, format_version: 1, brief: {} }),
    }),
    fetch(new URL(`/api/funnels/${funnelId}/service-brief`, baseUrl), {
      method: 'PUT',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ expected_version: 4, format_version: 2, brief: {} }),
    }),
    fetch(new URL(`/api/funnels/${funnelId}/service-brief`, baseUrl), { method: 'DELETE' }),
    fetch(new URL(`/api/funnels/${funnelId}/service-brief`, baseUrl), {
      method: 'PUT',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ expected_version: 4, format_version: 1, brief: { audience: 'x'.repeat(41_000) } }),
    }),
  ];
  const expectedStatuses = [403, 400, 400, 405, 413];
  const errorResponses = await Promise.all(errorRequests);
  for (let index = 0; index < errorResponses.length; index += 1) {
    expect(errorResponses[index].status).toBe(expectedStatuses[index]);
    expect(errorResponses[index].headers.get('cache-control')).toContain('private');
    expect(errorResponses[index].headers.get('cache-control')).toContain('no-store');
  }
  const customHostResponse = await new Promise((resolve, reject) => {
    const request = http.request(
      new URL(`/api/funnels/${funnelId}/service-brief`, baseUrl),
      { method: 'GET', headers: { host: 'custom.example' } },
      response => {
        response.resume();
        response.on('end', () => resolve(response));
      },
    );
    request.on('error', reject);
    request.end();
  });
  expect(customHostResponse.statusCode).toBe(404);
  expect(customHostResponse.headers['cache-control']).toContain('private');
  expect(customHostResponse.headers['cache-control']).toContain('no-store');

  await page.setViewportSize({ width: 390, height: 844 });
  const briefWidth = await page.locator('.service-brief-page').evaluate(element => element.scrollWidth);
  expect(briefWidth).toBeLessThanOrEqual(390);
  await page.addScriptTag({ content: axeCore.source });
  const accessibility = await page.evaluate(async () => {
    const result = await axe.run(document.querySelector('.service-brief-page'));
    return result.violations.map(({ id, impact, description }) => ({ id, impact, description }));
  });
  expect(accessibility).toEqual([]);

  console.log('PUB-005 browser: private notes, keyboard intention selection, save/reload, conflict preservation and comparison, explicit overwrite, worst-case escaped payload, retry idempotence, no-store errors, custom-host isolation, reporting separation, mobile layout, and axe checks passed');
} finally {
  await browser.close();
}
