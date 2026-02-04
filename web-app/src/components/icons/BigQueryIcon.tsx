interface BigQueryIconProps {
  className?: string;
  width?: number;
  height?: number;
}

export const BigQueryIcon = ({ className, width = 32, height = 32 }: BigQueryIconProps) => {
  return (
    <svg
      width={width}
      height={height}
      viewBox='0 0 24 24'
      fill='none'
      xmlns='http://www.w3.org/2000/svg'
      className={className}
    >
      <g clip-path='url(#a)'>
        <path
          d='M5.43 21.823.21 12.78a1.56 1.56 0 0 1 0-1.563l5.22-9.042a1.56 1.56 0 0 1 1.35-.78h10.446a1.56 1.56 0 0 1 1.344.78l5.22 9.042a1.56 1.56 0 0 1 0 1.563l-5.22 9.042a1.56 1.56 0 0 1-1.35.78H6.777a1.56 1.56 0 0 1-1.347-.78'
          fill='#4386FA'
        />
        <path
          opacity='.1'
          d='M15.261 9.088s1.452 3.48-.528 5.454c-1.978 1.972-5.58.711-5.58.711s5.355 5.425 7.327 7.348h.744a1.56 1.56 0 0 0 1.35-.782l3.456-5.985z'
          fill='#000'
        />
        <path
          d='m16.976 16.212-1.605-1.605a.3.3 0 0 0-.058-.045 4.361 4.361 0 1 0-.758.765.3.3 0 0 0 .042.055l1.605 1.605a.25.25 0 0 0 .354 0l.42-.42a.25.25 0 0 0 0-.357m-5.111-1.038a3.28 3.28 0 1 1 0-6.563 3.28 3.28 0 0 1 0 6.563'
          fill='#fff'
        />
        <path d='M9.768 11.718v1.356c.209.369.51.675.876.891v-2.256z' fill='#fff' />
        <path
          d='M9.768 11.718v1.356c.209.369.51.675.876.891v-2.256zm1.639-1.125v3.665c.291.054.588.054.877 0v-3.665z'
          fill='#fff'
        />
        <path
          d='M11.408 10.593v3.665c.291.054.588.054.877 0v-3.665zm2.537 2.478v-.801h-.876v1.691a2.4 2.4 0 0 0 .876-.888'
          fill='#fff'
        />
        <path d='M13.944 13.071v-.801h-.876v1.691a2.4 2.4 0 0 0 .876-.888' fill='#fff' />
      </g>
    </svg>
  );
};

export default BigQueryIcon;
