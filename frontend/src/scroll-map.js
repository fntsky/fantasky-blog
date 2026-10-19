// Interpolate between measured source and rendered block positions.
export function mapPosition(points, value, from = 'source', to = 'preview') {
  if (!points.length) return 0
  if (value <= points[0][from]) return points[0][to]
  for (let index = 1; index < points.length; index += 1) {
    const previous = points[index - 1]
    const next = points[index]
    if (value <= next[from]) {
      const distance = next[from] - previous[from]
      return distance > 0 ? previous[to] + (next[to] - previous[to]) * (value - previous[from]) / distance : next[to]
    }
  }
  return points.at(-1)[to]
}
