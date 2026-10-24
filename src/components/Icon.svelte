<script lang="ts">
  let {name,size=20}:{name:string;size?:number}=$props();
  const paths:Record<string,string>={
    terminal:'M4 5h16v14H4z M7 9l3 3-3 3 M12 15h4',browser:'M21 12a9 9 0 1 1-18 0 9 9 0 0 1 18 0 M3 12h18 M12 3c5 5 5 13 0 18-5-5-5-13 0-18',
    files:'M3 7V5h7l2 3h9v12H3z',settings:'M12 8a4 4 0 1 1 0 8 4 4 0 0 1 0-8 M12 2v3 M12 19v3 M2 12h3 M19 12h3 M5 5l2 2 M17 17l2 2 M5 19l2-2 M17 7l2-2',
    downloads:'M12 3v12 M7 10l5 5 5-5 M4 16v5h16v-5',close:'M6 6l12 12 M6 18L18 6',min:'M5 16h14',max:'M5 5h14v14H5z',restore:'M8 5V3h13v13h-2 M3 8h13v13H3z',
    back:'M14 5l-7 7 7 7',forward:'M10 5l7 7-7 7',up:'M5 12l7-7 7 7 M12 5v15',refresh:'M20 11a8 8 0 1 0-2 7 M20 4v7h-7',lock:'M7 10V7a5 5 0 0 1 10 0v3 M5 10h14v11H5z',
    file:'M5 3h9l5 5v13H5z M14 3v6h5',plus:'M12 5v14 M5 12h14',power:'M12 2v10 M6 5a9 9 0 1 0 12 0',check:'M4 12l5 5L20 6',edit:'M4 16L16 4l4 4L8 20H4z',trash:'M3 6h18 M9 6V3h6v3 M5 6l1 15h12l1-15 M10 10v7 M14 10v7',
    copy:'M8 8h13v13H8z M16 8V3H3v13h5',move:'M12 2v20 M2 12h20 M8 6l4-4 4 4 M8 18l4 4 4-4 M6 8l-4 4 4 4 M18 8l4 4-4 4',save:'M3 3h15l3 3v15H3z M7 3v6h10V3 M7 21v-8h10v8',
    launcher:'M4 4h6v6H4z M14 4h6v6h-6z M4 14h6v6H4z M14 14h6v6h-6z',server:'M3 3h18v7H3z M3 14h18v7H3z M6 6h1 M6 17h1',image:'M3 3h18v18H3z M3 17l6-6 4 4 3-3 5 5 M16 7h.01',home:'M3 10l9-7 9 7 M5 9v12h14V9 M9 21v-8h6v8',search:'M16 16l5 5 M18 10a8 8 0 1 1-16 0 8 8 0 0 1 16 0',stop:'M5 5h14v14H5z',link:'M8 15l7-7 M9 6l3-3a5 5 0 0 1 7 7l-3 3 M15 18l-3 3a5 5 0 0 1-7-7l3-3'
  };
</script>
<svg width={size} height={size} viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d={paths[name]||paths.file}/></svg>
