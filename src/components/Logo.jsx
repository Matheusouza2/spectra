import symbol from '../assets/spectra-symbol.png';

export default function Logo({ size = 28, className = '' }) {
  return (
    <img
      className={className}
      src={symbol}
      height={size}
      style={{ height: size, width: 'auto' }}
      alt=""
      aria-hidden="true"
      draggable="false"
    />
  );
}
