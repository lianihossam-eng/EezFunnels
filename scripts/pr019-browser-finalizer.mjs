export async function finalizePr019Browser(closeBrowser, worker) {
  let browserError;
  let browserFailed = false;
  try {
    await closeBrowser();
  } catch (error) {
    browserError = error;
    browserFailed = true;
  }

  let workerError;
  let workerFailed = false;
  try {
    if (worker) await worker.stop();
  } catch (error) {
    workerError = error;
    workerFailed = true;
  }

  if (browserFailed && workerFailed) {
    throw new AggregateError([browserError, workerError], "PR-019 browser and Worker cleanup both failed");
  }
  if (browserFailed) throw browserError;
  if (workerFailed) throw workerError;
}
