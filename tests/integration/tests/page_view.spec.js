import { test, expect } from '@playwright/test';

test('captures a page view and returns as a metric', async ({ page }) => {
  await Promise.all([
    page.waitForRequest(request => request.url().includes('31003') && request.method() === 'POST'),
    page.goto('http://localhost:3000/')
  ])

  const metrics = await page.goto('http://localhost:31004/metrics')
  const results = await metrics.body()
  
  expect(results.toString()).toContain('entity="page",action="view",app_id="integration-tests"');
});
