import './style.css'
export const metadata = {
  title: 'Meeting Notes Server',
  description: 'Your recordings, tasks, and transcripts — durably together.',
}
export default function Layout({ children }: { children: React.ReactNode }) {
  return (
    <html lang="en">
      <body>{children}</body>
    </html>
  )
}
