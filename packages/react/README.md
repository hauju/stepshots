# @stepshots/react

React component for embedding interactive [Stepshots](https://stepshots.com) demos.

## Installation

```sh
npm install @stepshots/react
```

## Usage

```tsx
import { StepshotsDemo } from "@stepshots/react";

function App() {
  return <StepshotsDemo demoId="your-demo-id" />;
}
```

## Props

| Prop | Type | Default | Description |
|------|------|---------|-------------|
| `demoId` | `string` | **required** | The ID of the demo to embed |
| `baseUrl` | `string` | `"https://app.stepshots.com"` | Base URL of the Stepshots app |
| `autoplay` | `boolean` | `false` | Auto-play the demo on load |
| `theme` | `"light" \| "dark"` | — | Force a color theme |
| `start` | `number` | — | Start at a specific step (1-indexed) |
| `hideControls` | `boolean` | `false` | Hide playback controls |
| `width` | `string \| number` | `"100%"` | Container width |
| `aspectRatio` | `string` | `"16/9"` | Container aspect ratio |
| `style` | `React.CSSProperties` | — | Additional container styles |
| `className` | `string` | — | CSS class for the container |

## License

MIT
