export function BrandIcon() {
  return <img src="/gday-meetings.png" alt="Gday Meetings" width={32} height={32} />
}

export function BrandLogo() {
  return (
    <div style={{ display: 'flex', alignItems: 'center', gap: 12 }}>
      <img src="/gday-meetings.png" alt="" width={72} height={72} />
      <span style={{ fontSize: 24, fontWeight: 600 }}>Gday Meetings</span>
    </div>
  )
}
