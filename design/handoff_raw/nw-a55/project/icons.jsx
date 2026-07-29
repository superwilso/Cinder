// Shared SVG icons — sized via fontSize / color via currentColor
// Each is wrapped in a span so callers can size with width:1em.

const Icon = ({ children, size = 18, ...rest }) => (
  <svg width={size} height={size} viewBox="0 0 24 24" fill="none" stroke="currentColor"
       strokeWidth="1.6" strokeLinecap="round" strokeLinejoin="round" {...rest}>
    {children}
  </svg>
);

const IconPlay = (p) => <Icon {...p}><polygon points="7 5 19 12 7 19 7 5" fill="currentColor" stroke="none"/></Icon>;
const IconPause = (p) => <Icon {...p}><rect x="7" y="5" width="3.5" height="14" fill="currentColor" stroke="none"/><rect x="13.5" y="5" width="3.5" height="14" fill="currentColor" stroke="none"/></Icon>;
const IconPrev = (p) => <Icon {...p}><polygon points="18 5 8 12 18 19 18 5" fill="currentColor" stroke="none"/><rect x="5" y="5" width="2" height="14" fill="currentColor" stroke="none"/></Icon>;
const IconNext = (p) => <Icon {...p}><polygon points="6 5 16 12 6 19 6 5" fill="currentColor" stroke="none"/><rect x="17" y="5" width="2" height="14" fill="currentColor" stroke="none"/></Icon>;
const IconHeart = (p) => <Icon {...p}><path d="M20.84 4.61a5.5 5.5 0 0 0-7.78 0L12 5.67l-1.06-1.06a5.5 5.5 0 0 0-7.78 7.78l1.06 1.06L12 21.23l7.78-7.78 1.06-1.06a5.5 5.5 0 0 0 0-7.78z"/></Icon>;
const IconHeartFill = (p) => <Icon {...p}><path d="M20.84 4.61a5.5 5.5 0 0 0-7.78 0L12 5.67l-1.06-1.06a5.5 5.5 0 0 0-7.78 7.78l1.06 1.06L12 21.23l7.78-7.78 1.06-1.06a5.5 5.5 0 0 0 0-7.78z" fill="currentColor"/></Icon>;
const IconShuffle = (p) => <Icon {...p}><polyline points="16 3 21 3 21 8"/><line x1="4" y1="20" x2="21" y2="3"/><polyline points="21 16 21 21 16 21"/><line x1="15" y1="15" x2="21" y2="21"/><line x1="4" y1="4" x2="9" y2="9"/></Icon>;
const IconRepeat = (p) => <Icon {...p}><polyline points="17 1 21 5 17 9"/><path d="M3 11V9a4 4 0 0 1 4-4h14"/><polyline points="7 23 3 19 7 15"/><path d="M21 13v2a4 4 0 0 1-4 4H3"/></Icon>;
const IconMore = (p) => <Icon {...p}><circle cx="5" cy="12" r="1.4" fill="currentColor" stroke="none"/><circle cx="12" cy="12" r="1.4" fill="currentColor" stroke="none"/><circle cx="19" cy="12" r="1.4" fill="currentColor" stroke="none"/></Icon>;
const IconSearch = (p) => <Icon {...p}><circle cx="11" cy="11" r="7"/><line x1="21" y1="21" x2="16.6" y2="16.6"/></Icon>;
const IconBack = (p) => <Icon {...p}><polyline points="15 18 9 12 15 6"/></Icon>;
const IconQueue = (p) => <Icon {...p}><line x1="8" y1="6" x2="21" y2="6"/><line x1="8" y1="12" x2="21" y2="12"/><line x1="8" y1="18" x2="14" y2="18"/><circle cx="4" cy="6" r="1" fill="currentColor"/><circle cx="4" cy="12" r="1" fill="currentColor"/><circle cx="4" cy="18" r="1" fill="currentColor"/></Icon>;
const IconLock = (p) => <Icon {...p}><rect x="4" y="11" width="16" height="10" rx="2"/><path d="M8 11V7a4 4 0 0 1 8 0v4"/></Icon>;
const IconWifi = (p) => <Icon {...p}><path d="M5 12.55a11 11 0 0 1 14 0"/><path d="M2 8.82a15 15 0 0 1 20 0"/><path d="M8.5 16.43a6 6 0 0 1 7 0"/><line x1="12" y1="20" x2="12" y2="20"/></Icon>;
const IconBluetooth = (p) => <Icon {...p}><polyline points="6.5 6.5 17.5 17.5 12 23 12 1 17.5 6.5 6.5 17.5"/></Icon>;
const IconCheck = (p) => <Icon {...p}><polyline points="20 6 9 17 4 12"/></Icon>;
const IconChevron = (p) => <Icon {...p}><polyline points="9 18 15 12 9 6"/></Icon>;
const IconHeadphone = (p) => <Icon {...p}><path d="M3 18v-6a9 9 0 0 1 18 0v6"/><path d="M21 19a2 2 0 0 1-2 2h-1a2 2 0 0 1-2-2v-3a2 2 0 0 1 2-2h3zM3 19a2 2 0 0 0 2 2h1a2 2 0 0 0 2-2v-3a2 2 0 0 0-2-2H3z"/></Icon>;
const IconSlider = (p) => <Icon {...p}><line x1="4" y1="21" x2="4" y2="14"/><line x1="4" y1="10" x2="4" y2="3"/><line x1="12" y1="21" x2="12" y2="12"/><line x1="12" y1="8" x2="12" y2="3"/><line x1="20" y1="21" x2="20" y2="16"/><line x1="20" y1="12" x2="20" y2="3"/><line x1="1" y1="14" x2="7" y2="14"/><line x1="9" y1="8" x2="15" y2="8"/><line x1="17" y1="16" x2="23" y2="16"/></Icon>;
const IconVolume = (p) => <Icon {...p}><polygon points="11 5 6 9 2 9 2 15 6 15 11 19 11 5" fill="currentColor" stroke="none"/><path d="M15.5 8.5a5 5 0 0 1 0 7"/><path d="M19 5a9 9 0 0 1 0 14"/></Icon>;
const IconList = (p) => <Icon {...p}><line x1="3" y1="6" x2="21" y2="6"/><line x1="3" y1="12" x2="21" y2="12"/><line x1="3" y1="18" x2="21" y2="18"/></Icon>;
const IconGrid = (p) => <Icon {...p}><rect x="3" y="3" width="7" height="7"/><rect x="14" y="3" width="7" height="7"/><rect x="3" y="14" width="7" height="7"/><rect x="14" y="14" width="7" height="7"/></Icon>;
const IconGrid3 = (p) => <Icon {...p}><rect x="3" y="3" width="4.5" height="4.5"/><rect x="9.75" y="3" width="4.5" height="4.5"/><rect x="16.5" y="3" width="4.5" height="4.5"/><rect x="3" y="9.75" width="4.5" height="4.5"/><rect x="9.75" y="9.75" width="4.5" height="4.5"/><rect x="16.5" y="9.75" width="4.5" height="4.5"/><rect x="3" y="16.5" width="4.5" height="4.5"/><rect x="9.75" y="16.5" width="4.5" height="4.5"/><rect x="16.5" y="16.5" width="4.5" height="4.5"/></Icon>;
const IconUndo = (p) => <Icon {...p}><polyline points="9 14 4 9 9 4"/><path d="M4 9h11a5 5 0 0 1 5 5v0a5 5 0 0 1-5 5H9"/></Icon>;
const IconRedo = (p) => <Icon {...p}><polyline points="15 14 20 9 15 4"/><path d="M20 9H9a5 5 0 0 0-5 5v0a5 5 0 0 0 5 5h6"/></Icon>;
const IconShelf = (p) => <Icon {...p}><rect x="3" y="4" width="18" height="5" rx="1"/><rect x="3" y="11" width="18" height="9" rx="1"/><line x1="7.5" y1="14" x2="7.5" y2="17"/><line x1="11" y1="14" x2="11" y2="17"/></Icon>;
const IconBookmark = (p) => <Icon {...p}><path d="M6 3h12a1 1 0 0 1 1 1v17l-7-4-7 4V4a1 1 0 0 1 1-1z"/></Icon>;
const IconBookmarkFill = (p) => <Icon {...p}><path d="M6 3h12a1 1 0 0 1 1 1v17l-7-4-7 4V4a1 1 0 0 1 1-1z" fill="currentColor"/></Icon>;
const IconClose = (p) => <Icon {...p}><line x1="6" y1="6" x2="18" y2="18"/><line x1="18" y1="6" x2="6" y2="18"/></Icon>;
const IconPlus = (p) => <Icon {...p}><line x1="12" y1="5" x2="12" y2="19"/><line x1="5" y1="12" x2="19" y2="12"/></Icon>;
const IconSun = (p) => <Icon {...p}><circle cx="12" cy="12" r="4.2"/><line x1="12" y1="2" x2="12" y2="4.5"/><line x1="12" y1="19.5" x2="12" y2="22"/><line x1="2" y1="12" x2="4.5" y2="12"/><line x1="19.5" y1="12" x2="22" y2="12"/><line x1="4.9" y1="4.9" x2="6.7" y2="6.7"/><line x1="17.3" y1="17.3" x2="19.1" y2="19.1"/><line x1="4.9" y1="19.1" x2="6.7" y2="17.3"/><line x1="17.3" y1="6.7" x2="19.1" y2="4.9"/></Icon>;
const IconMoon = (p) => <Icon {...p}><path d="M21 12.8A9 9 0 1 1 11.2 3a7 7 0 0 0 9.8 9.8z" fill="currentColor" stroke="none"/></Icon>;
const IconUser = (p) => <Icon {...p}><circle cx="12" cy="8" r="4"/><path d="M4 21v-1a7 7 0 0 1 14 0v1"/></Icon>;
const IconClock = (p) => <Icon {...p}><circle cx="12" cy="12" r="9"/><polyline points="12 7 12 12 16 14"/></Icon>;

Object.assign(window, {
  Icon, IconPlay, IconPause, IconPrev, IconNext, IconHeart, IconHeartFill,
  IconShuffle, IconRepeat, IconMore, IconSearch, IconBack, IconQueue,
  IconLock, IconWifi, IconBluetooth, IconCheck, IconChevron, IconHeadphone,
  IconSlider, IconVolume, IconList, IconGrid, IconGrid3,
  IconUndo, IconRedo, IconShelf, IconBookmark, IconBookmarkFill, IconClose,
  IconPlus, IconSun, IconMoon, IconUser, IconClock,
});
