export function InvoicesTableHeader() {
  return (
    <thead className='border-b text-muted-foreground'>
      <tr>
        <th className='px-3 py-2 text-left font-normal text-xs'>Date</th>
        <th className='px-3 py-2 text-left font-normal text-xs'>Due</th>
        <th className='px-3 py-2 text-right font-normal text-xs'>Total</th>
        <th className='px-3 py-2 text-left font-normal text-xs'>Status</th>
        <th className='px-3 py-2 text-right font-normal text-xs'>Actions</th>
      </tr>
    </thead>
  );
}
