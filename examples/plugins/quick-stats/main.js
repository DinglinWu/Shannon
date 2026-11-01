Shannon.register({
  summarize({values}) {
    const sorted = [...values].sort((a, b) => a - b);
    const sum = values.reduce((total, value) => total + value, 0);
    const middle = Math.floor(sorted.length / 2);
    return {
      count: values.length, sum, mean: sum / values.length,
      min: sorted[0], max: sorted.at(-1),
      median: sorted.length % 2 ? sorted[middle] : (sorted[middle - 1] + sorted[middle]) / 2,
    };
  },
});
