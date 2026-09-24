import './style.css'
export const metadata = {
  title: 'Gday Meetings Server',
  description: 'Your recordings, tasks, and transcripts — durably together.',
  icons: { icon: '/gday-meetings.png', apple: '/gday-meetings.png' },
}
export default function Layout({ children }: { children: React.ReactNode }) {
  return (
    <html lang="en">
      <body>{children}</body>
    </html>
  )
}
