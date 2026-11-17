import React, { useEffect, useRef, useState, useMemo } from 'react';
import * as d3 from 'd3';
import * as topojson from 'topojson-client';
import regionsData from 'morocco-map/data/regions.json';
import { University, Institution, Language } from '../types';
import { translations } from '../data/translations';
import { MapPin, Building2, GraduationCap, RotateCcw, ZoomIn, ZoomOut, Compass, ExternalLink, Layers } from 'lucide-react';

interface CityData {
  id: string;
  name_ar: string;
  name_fr: string;
  region_id: string;
  region_ar: string;
  region_fr: string;
  coords: [number, number]; // [lon, lat]
  universities: University[];
  institutions: Institution[];
}

interface Props {
  universities: University[];
  institutions: Institution[];
  selectedCity: string;
  onSelectCity: (city: string) => void;
  onSelectInstitution: (inst: Institution) => void;
  onSelectUniversity: (univ: University) => void;
  lang: Language;
}

// Coordinate mappings for academic centers located within Morocco's 12 administrative regions
const ACADEMIC_HUBS: { [key: string]: { coords: [number, number]; region_id: string; region_ar: string; region_fr: string } } = {
  'الرباط': { coords: [-6.8416, 34.0209], region_id: 'MA-04', region_ar: 'الرباط-سلا-القنيطرة', region_fr: 'Rabat-Salé-Kénitra' },
  'الدار البيضاء': { coords: [-7.5898, 33.5731], region_id: 'MA-06', region_ar: 'الدار البيضاء-سطات', region_fr: 'Casablanca-Settat' },
  'مراكش': { coords: [-7.9811, 31.6295], region_id: 'MA-07', region_ar: 'مراكش-أسفي', region_fr: 'Marrakech-Safi' },
  'فاس': { coords: [-5.0003, 34.0331], region_id: 'MA-03', region_ar: 'فاس-مكناس', region_fr: 'Fès-Meknès' },
  'طنجة': { coords: [-5.8340, 35.7595], region_id: 'MA-01', region_ar: 'طنجة تطوان الحسيمة', region_fr: 'Tanger-Tétouan-Al Hoceïma' },
  'تطوان': { coords: [-5.3684, 35.5785], region_id: 'MA-01', region_ar: 'طنجة تطوان الحسيمة', region_fr: 'Tanger-Tétouan-Al Hoceïma' },
  'أكادير': { coords: [-9.5981, 30.4278], region_id: 'MA-09', region_ar: 'سوس-ماسة', region_fr: 'Souss-Massa' },
  'تارودانت': { coords: [-8.8770, 30.4703], region_id: 'MA-09', region_ar: 'سوس-ماسة', region_fr: 'Souss-Massa' },
  'وجدة': { coords: [-1.9114, 34.6867], region_id: 'MA-02', region_ar: 'الشرقية', region_fr: 'L’Oriental' },
  'بني ملال': { coords: [-6.3533, 32.3394], region_id: 'MA-05', region_ar: 'بني ملال-خنيفرة', region_fr: 'Béni Mellal-Khénifra' },
  'العيون': { coords: [-13.2033, 27.1536], region_id: 'MA-11', region_ar: 'العيون الساقية الحمراء', region_fr: 'Laâyoune-Sakia El Hamra' },
  'الداخلة': { coords: [-15.9388, 23.7136], region_id: 'MA-12', region_ar: 'الداخلة-وادي الذهب', region_fr: 'Dakhla-Oued Ed-Dahab' },
  'الرشيدية': { coords: [-4.4285, 31.9315], region_id: 'MA-08', region_ar: 'درعة تافيلالت', region_fr: 'Drâa-Tafilalet' },
  'كلميم': { coords: [-10.0574, 28.9869], region_id: 'MA-10', region_ar: 'كلميم وادي نون', region_fr: 'Guelmim-Oued Noun' },
};

export const InteractiveMap: React.FC<Props> = ({
  universities,
  institutions,
  selectedCity,
  onSelectCity,
  onSelectInstitution,
  onSelectUniversity,
  lang,
}) => {
  const svgRef = useRef<SVGSVGElement | null>(null);
  const containerRef = useRef<HTMLDivElement | null>(null);

  const [hoveredRegion, setHoveredRegion] = useState<{ id: string; name_ar: string; name_en: string; count: number } | null>(null);
  const [hoveredCity, setHoveredCity] = useState<CityData | null>(null);
  const [activeCityDetail, setActiveCityDetail] = useState<CityData | null>(null);
  const [selectedRegionId, setSelectedRegionId] = useState<string | null>(null);

  const t = translations[lang];
  const isAr = lang === 'ar';

  // Convert TopoJSON to GeoJSON FeatureCollection using yousfiSaad/morocco-map data
  const geojsonRegions: any = useMemo(() => {
    try {
      const topo: any = regionsData;
      const objectKey = Object.keys(topo.objects)[0];
      return topojson.feature(topo, topo.objects[objectKey]);
    } catch (err) {
      console.error('Error parsing TopoJSON from morocco-map:', err);
      return { type: 'FeatureCollection', features: [] };
    }
  }, []);

  // Map city data with universities and institutions
  const cityDataList: CityData[] = useMemo(() => {
    const map = new Map<string, CityData>();

    Object.entries(ACADEMIC_HUBS).forEach(([cityName, meta]) => {
      map.set(cityName, {
        id: cityName,
        name_ar: cityName,
        name_fr: cityName === 'الرباط' ? 'Rabat' :
                 cityName === 'الدار البيضاء' ? 'Casablanca' :
                 cityName === 'مراكش' ? 'Marrakech' :
                 cityName === 'فاس' ? 'Fès' :
                 cityName === 'طنجة' ? 'Tanger' :
                 cityName === 'تطوان' ? 'Tétouan' :
                 cityName === 'أكادير' ? 'Agadir' :
                 cityName === 'تارودانت' ? 'Taroudant' :
                 cityName === 'وجدة' ? 'Oujda' :
                 cityName === 'بني ملال' ? 'Béni Mellal' :
                 cityName === 'العيون' ? 'Laâyoune' :
                 cityName === 'الداخلة' ? 'Dakhla' :
                 cityName === 'الرشيدية' ? 'Errachidia' :
                 cityName === 'كلميم' ? 'Guelmim' : cityName,
        region_id: meta.region_id,
        region_ar: meta.region_ar,
        region_fr: meta.region_fr,
        coords: meta.coords,
        universities: [],
        institutions: [],
      });
    });

    universities.forEach((u) => {
      const city = u.city_ar;
      if (map.has(city)) {
        map.get(city)!.universities.push(u);
      }
    });

    institutions.forEach((inst) => {
      const city = inst.city_ar;
      if (map.has(city)) {
        map.get(city)!.institutions.push(inst);
      }
    });

    return Array.from(map.values());
  }, [universities, institutions]);

  // Count institutions per region
  const regionCounts = useMemo(() => {
    const counts: { [regionId: string]: number } = {};
    cityDataList.forEach((c) => {
      const total = c.universities.length + c.institutions.length;
      counts[c.region_id] = (counts[c.region_id] || 0) + total;
    });
    return counts;
  }, [cityDataList]);

  // Keep active city synced with prop
  useEffect(() => {
    if (selectedCity && selectedCity !== 'all') {
      const found = cityDataList.find((c) => c.name_ar === selectedCity || c.name_fr === selectedCity);
      if (found) {
        setActiveCityDetail(found);
        setSelectedRegionId(found.region_id);
      }
    } else {
      setActiveCityDetail(null);
      setSelectedRegionId(null);
    }
  }, [selectedCity, cityDataList]);

  // D3 rendering lifecycle with yousfiSaad/morocco-map TopoJSON
  useEffect(() => {
    if (!svgRef.current || !geojsonRegions || !geojsonRegions.features.length) return;

    const width = 800;
    const height = 540;

    const svg = d3.select(svgRef.current);
    svg.selectAll('*').remove();

    // Map container with zoom
    const mapGroup = svg.append('g').attr('class', 'morocco-map-container');

    // Defs for filters and gradients
    const defs = svg.append('defs');

    // Drop shadow filter for active regions
    const filter = defs.append('filter')
      .attr('id', 'region-shadow')
      .attr('x', '-20%')
      .attr('y', '-20%')
      .attr('width', '140%')
      .attr('height', '140%');
    filter.append('feDropShadow')
      .attr('dx', '0')
      .attr('dy', '3')
      .attr('stdDeviation', '4')
      .attr('flood-opacity', '0.12');

    // Create D3 Mercator projection fitted strictly to Morocco's 12 administrative regions
    const projection = d3.geoMercator()
      .fitExtent([[30, 25], [width - 30, height - 25]], geojsonRegions);

    const pathGenerator = d3.geoPath().projection(projection);

    // Ocean backdrop
    svg.insert('rect', ':first-child')
      .attr('width', width)
      .attr('height', height)
      .attr('fill', '#F5F7F6')
      .attr('rx', 16);

    // Subtle geographic meridians
    const graticuleGroup = mapGroup.append('g').attr('class', 'graticules').attr('opacity', 0.2);
    [-15, -10, -5].forEach((lon) => {
      const p1 = projection([lon, 20]);
      const p2 = projection([lon, 36]);
      if (p1 && p2) {
        graticuleGroup.append('line')
          .attr('x1', p1[0]).attr('y1', p1[1])
          .attr('x2', p2[0]).attr('y2', p2[1])
          .attr('stroke', '#B2C4B8')
          .attr('stroke-width', 0.8)
          .attr('stroke-dasharray', '3,3');
      }
    });

    // 1. Draw 12 Moroccan Regions from yousfiSaad/morocco-map
    const regionsGroup = mapGroup.append('g').attr('class', 'regions-layer');

    regionsGroup.selectAll('path')
      .data(geojsonRegions.features)
      .enter()
      .append('path')
      .attr('d', pathGenerator as any)
      .attr('id', (d: any) => `region-${d.id}`)
      .attr('class', 'region-path cursor-pointer transition-colors duration-150')
      .attr('fill', (d: any) => {
        const hasContent = (regionCounts[d.id] || 0) > 0;
        const isSelected = selectedRegionId === d.id;
        if (isSelected) return '#E2EBE5'; // active selected region
        if (hasContent) return '#FAF6EE'; // region with verified universities
        return '#F0EDE6'; // subtle default region
      })
      .attr('stroke', (d: any) => (selectedRegionId === d.id ? '#143D30' : '#D1C7B3'))
      .attr('stroke-width', (d: any) => (selectedRegionId === d.id ? 1.8 : 0.9))
      .attr('stroke-linejoin', 'round')
      .on('mouseenter', function (event, d: any) {
        d3.select(this)
          .attr('fill', '#E5EEE8')
          .attr('stroke', '#143D30')
          .attr('stroke-width', 1.5);

        setHoveredRegion({
          id: d.id,
          name_ar: d.properties['name:ar'] || d.properties['name'] || '',
          name_en: d.properties['name:en'] || d.properties['name'] || '',
          count: regionCounts[d.id] || 0,
        });
      })
      .on('mouseleave', function (event, d: any) {
        const isSelected = selectedRegionId === d.id;
        const hasContent = (regionCounts[d.id] || 0) > 0;
        d3.select(this)
          .attr('fill', isSelected ? '#E2EBE5' : hasContent ? '#FAF6EE' : '#F0EDE6')
          .attr('stroke', isSelected ? '#143D30' : '#D1C7B3')
          .attr('stroke-width', isSelected ? 1.8 : 0.9);

        setHoveredRegion(null);
      })
      .on('click', function (event, d: any) {
        setSelectedRegionId(d.id);
        // Find cities in this region
        const citiesInRegion = cityDataList.filter((c) => c.region_id === d.id && (c.universities.length > 0 || c.institutions.length > 0));
        if (citiesInRegion.length > 0) {
          onSelectCity(citiesInRegion[0].name_ar);
          setActiveCityDetail(citiesInRegion[0]);
        }
      });

    // 2. Overlay Academic Cities & Institution Markers
    const citiesGroup = mapGroup.append('g').attr('class', 'cities-layer');

    cityDataList.forEach((city) => {
      const projected = projection(city.coords);
      if (!projected) return;
      const [x, y] = projected;

      const hasInstitutions = city.universities.length > 0 || city.institutions.length > 0;
      const totalCount = city.universities.length + city.institutions.length;
      const isSelected = selectedCity === city.name_ar || selectedCity === city.name_fr;

      const cityG = citiesGroup.append('g')
        .attr('transform', `translate(${x}, ${y})`)
        .attr('class', 'cursor-pointer group')
        .on('mouseenter', () => setHoveredCity(city))
        .on('mouseleave', () => setHoveredCity(null))
        .on('click', () => {
          onSelectCity(city.name_ar);
          setActiveCityDetail(city);
          setSelectedRegionId(city.region_id);
        });

      // Active pulsing aura
      if (isSelected) {
        cityG.append('circle')
          .attr('r', 16)
          .attr('fill', '#143D30')
          .attr('opacity', 0.2)
          .attr('class', 'animate-ping');
      }

      // Base marker aura
      if (hasInstitutions) {
        cityG.append('circle')
          .attr('r', isSelected ? 11 : 8)
          .attr('fill', '#143D30')
          .attr('opacity', isSelected ? 0.25 : 0.15);
      }

      // Center Pin Point
      cityG.append('circle')
        .attr('r', isSelected ? 6.5 : hasInstitutions ? 5 : 3.5)
        .attr('fill', isSelected ? '#143D30' : hasInstitutions ? '#143D30' : '#8A928D')
        .attr('stroke', '#FFFFFF')
        .attr('stroke-width', isSelected ? 2.5 : 1.5);

      // Total count indicator badge
      if (hasInstitutions && totalCount > 0) {
        cityG.append('circle')
          .attr('cx', 6)
          .attr('cy', -6)
          .attr('r', 4.5)
          .attr('fill', isSelected ? '#B43425' : '#143D30')
          .attr('stroke', '#FFFFFF')
          .attr('stroke-width', 1);

        cityG.append('text')
          .attr('x', 6)
          .attr('y', -3.8)
          .attr('text-anchor', 'middle')
          .attr('fill', '#FFFFFF')
          .attr('font-size', '6.5px')
          .attr('font-weight', 'bold')
          .attr('font-family', 'var(--font-mono)')
          .text(totalCount);
      }

      // City Text Label
      const cityName = isAr ? city.name_ar : city.name_fr;
      const isWest = city.coords[0] < -8.0;

      cityG.append('text')
        .attr('x', isWest ? 9 : -9)
        .attr('y', 3.5)
        .attr('text-anchor', isWest ? 'start' : 'end')
        .attr('fill', isSelected ? '#143D30' : hasInstitutions ? '#1A1816' : '#57605A')
        .attr('font-weight', isSelected || hasInstitutions ? '700' : '500')
        .attr('font-size', isSelected ? '11px' : hasInstitutions ? '9.5px' : '8px')
        .attr('paint-order', 'stroke')
        .attr('stroke', '#FAF7F0')
        .attr('stroke-width', 2.8)
        .attr('stroke-linejoin', 'round')
        .text(cityName);
    });

    // Zoom and pan setup
    const zoom = d3.zoom<SVGSVGElement, unknown>()
      .scaleExtent([1, 4.5])
      .on('zoom', (event) => {
        mapGroup.attr('transform', event.transform);
      });

    svg.call(zoom);

  }, [geojsonRegions, cityDataList, selectedCity, selectedRegionId, isAr, regionCounts, onSelectCity]);

  // Reset map view
  const handleResetMap = () => {
    if (!svgRef.current) return;
    const svg = d3.select(svgRef.current);
    svg.transition().duration(400).call(d3.zoom<SVGSVGElement, unknown>().transform as any, d3.zoomIdentity);
    onSelectCity('all');
    setActiveCityDetail(null);
    setSelectedRegionId(null);
  };

  const handleZoom = (direction: 'in' | 'out') => {
    if (!svgRef.current) return;
    const svg = d3.select(svgRef.current);
    svg.transition().duration(250).call(
      d3.zoom<SVGSVGElement, unknown>().scaleBy as any,
      direction === 'in' ? 1.3 : 0.75
    );
  };

  return (
    <div className="bg-white rounded-2xl border border-stone-200 shadow-xs overflow-hidden mb-8 w-full max-w-full" ref={containerRef}>
      {/* Map Header Strip */}
      <div className="p-4 sm:p-5 bg-[#FAF7F0] border-b border-[#E7E2D6] flex flex-col sm:flex-row sm:items-center justify-between gap-3">
        <div className="flex items-center gap-2.5">
          <div className="w-8 h-8 rounded-lg bg-[#143D30] text-white flex items-center justify-center shrink-0">
            <Compass className="w-4 h-4" />
          </div>
          <div>
            <div className="flex items-center gap-2">
              <h3 className="text-sm sm:text-base font-bold text-stone-900 leading-snug">
                {isAr ? 'خريطة المغرب التفاعلية للجامعات (12 جهة)' : 'Carte interactive des 12 régions du Maroc'}
              </h3>
              <span className="text-[10px] font-mono px-2 py-0.5 rounded bg-stone-200/80 text-stone-700">
                yousfiSaad/morocco-map
              </span>
            </div>
            <p className="text-xs text-stone-500">
              {isAr
                ? 'انقر على أي جهة أو مدينة لاستكشاف كلياتها ومؤسساتها المعتمدة وتصفية الدليل المباشر'
                : 'Sélectionnez une région ou une ville pour afficher ses pôles académiques et filtrer l’annuaire'}
            </p>
          </div>
        </div>

        {/* Action buttons */}
        <div className="flex items-center gap-2 self-start sm:self-auto">
          <button
            onClick={() => handleZoom('in')}
            className="p-1.5 rounded-lg bg-white border border-stone-200 text-stone-700 hover:bg-stone-100 transition-colors"
            title="تكبير / Zoom in"
          >
            <ZoomIn className="w-4 h-4" />
          </button>
          <button
            onClick={() => handleZoom('out')}
            className="p-1.5 rounded-lg bg-white border border-stone-200 text-stone-700 hover:bg-stone-100 transition-colors"
            title="تصغير / Zoom out"
          >
            <ZoomOut className="w-4 h-4" />
          </button>
          <button
            onClick={handleResetMap}
            className="inline-flex items-center gap-1.5 px-3 py-1.5 rounded-lg bg-white border border-stone-200 text-xs font-medium text-stone-700 hover:bg-stone-100 transition-colors"
            title="إعادة ضبط العرض"
          >
            <RotateCcw className="w-3.5 h-3.5" />
            <span>{isAr ? 'إعادة ضبط' : 'Réinitialiser'}</span>
          </button>
        </div>
      </div>

      {/* Centered Morocco Map Container ("ثابتة في الوسط") */}
      <div className="relative bg-[#F5F7F6] overflow-hidden flex flex-col items-center justify-center p-3 sm:p-6 w-full max-w-full">
        {/* Moroccan Academic Quick Selector Chips (Centered above map) */}
        <div className="w-full max-w-3xl flex flex-wrap items-center justify-center gap-1.5 mb-4 text-xs">
          <span className="text-stone-500 font-semibold me-1">{isAr ? 'المدن الجامعية:' : 'Pôles :'}</span>
          {['الكل', 'الرباط', 'الدار البيضاء', 'مراكش', 'فاس', 'طنجة', 'أكادير', 'تارودانت', 'وجدة'].map((cityName) => {
            const isAll = cityName === 'الكل';
            const isSelected = isAll ? selectedCity === 'all' : selectedCity === cityName;
            return (
              <button
                key={cityName}
                onClick={() => {
                  if (isAll) {
                    onSelectCity('all');
                    setActiveCityDetail(null);
                    setSelectedRegionId(null);
                  } else {
                    onSelectCity(cityName);
                    const found = cityDataList.find((c) => c.name_ar === cityName);
                    if (found) {
                      setActiveCityDetail(found);
                      setSelectedRegionId(found.region_id);
                    }
                  }
                }}
                className={`px-3 py-1 rounded-full text-[11px] font-semibold transition-all ${
                  isSelected
                    ? 'bg-[#143D30] text-white shadow-xs'
                    : 'bg-white text-stone-700 hover:bg-stone-100 border border-stone-200'
                }`}
              >
                {cityName}
              </button>
            );
          })}
        </div>

        {/* Stable Centered SVG Canvas with aspect-video */}
        <div className="relative w-full max-w-3xl mx-auto flex items-center justify-center aspect-video overflow-hidden rounded-xl">
          <svg
            ref={svgRef}
            viewBox="0 0 800 540"
            className="w-full h-full max-h-full object-contain select-none touch-pan-x touch-pan-y mx-auto block drop-shadow-xs"
          />

          {/* Floating Hover Tooltip */}
          {(hoveredCity || hoveredRegion) && (
            <div
              className={`absolute top-4 ${isAr ? 'left-4' : 'right-4'} z-20 pointer-events-none bg-stone-900/90 text-white backdrop-blur-md px-3.5 py-2.5 rounded-xl shadow-lg border border-stone-700 text-xs max-w-xs animate-in fade-in duration-100`}
            >
              {hoveredCity ? (
                <>
                  <div className="flex items-center gap-1.5 font-bold mb-1">
                    <MapPin className="w-3.5 h-3.5 text-amber-400" />
                    <span>{isAr ? hoveredCity.name_ar : hoveredCity.name_fr}</span>
                  </div>
                  <p className="text-[11px] text-stone-300 mb-1.5">
                    {isAr ? hoveredCity.region_ar : hoveredCity.region_fr}
                  </p>
                  <div className="flex items-center gap-3 text-[11px] font-mono text-emerald-300">
                    <span>{hoveredCity.universities.length} جامعات</span>
                    <span aria-hidden="true">·</span>
                    <span>{hoveredCity.institutions.length} مدارس وكليات</span>
                  </div>
                </>
              ) : hoveredRegion ? (
                <>
                  <div className="flex items-center gap-1.5 font-bold mb-1">
                    <Layers className="w-3.5 h-3.5 text-emerald-400" />
                    <span>{isAr ? hoveredRegion.name_ar : hoveredRegion.name_en}</span>
                  </div>
                  <span className="font-mono text-[10px] text-stone-400 block mb-1">
                    رمز الجهة (ISO): {hoveredRegion.id}
                  </span>
                  <div className="text-[11px] font-mono text-emerald-300">
                    {hoveredRegion.count > 0 ? `${hoveredRegion.count} مؤسسات جامعية معتمدة` : 'قيد إضافة مؤسسات الجهة'}
                  </div>
                </>
              ) : null}
            </div>
          )}

          {/* Map Legend */}
          <div className={`absolute bottom-3 ${isAr ? 'right-3' : 'left-3'} z-10 bg-white/90 backdrop-blur-xs px-3 py-2 rounded-lg border border-stone-200/90 text-[11px] text-stone-600 space-y-1 shadow-xs`}>
            <div className="flex items-center gap-1.5">
              <span className="w-2.5 h-2.5 rounded-full bg-[#143D30]" />
              <span>{isAr ? 'قطب جامعي مدرج' : 'Pôle avec établissements'}</span>
            </div>
            <div className="flex items-center gap-1.5">
              <span className="w-2.5 h-2.5 rounded bg-[#FAF6EE] border border-[#D1C7B3]" />
              <span>{isAr ? 'الجهات الإدارية (12 جهة)' : '12 Régions officielles'}</span>
            </div>
          </div>
        </div>

        {/* Selected City or Region Details Box - Symmetrically Centered directly under the map */}
        {activeCityDetail && (
          <div className="w-full max-w-3xl mx-auto mt-5 p-4 sm:p-5 bg-white rounded-xl border border-stone-200 shadow-sm animate-in fade-in slide-in-from-top-2 duration-150">
            <div className="flex items-start justify-between pb-3 border-b border-stone-100 mb-4">
              <div className="flex items-center gap-2.5">
                <div className="w-8 h-8 rounded-lg bg-[#143D30] text-white flex items-center justify-center font-bold">
                  <MapPin className="w-4 h-4 text-amber-300" />
                </div>
                <div>
                  <h4 className="text-base font-bold text-stone-900 leading-tight">
                    {isAr ? activeCityDetail.name_ar : activeCityDetail.name_fr}
                  </h4>
                  <span className="text-xs text-stone-500 block">
                    {isAr ? activeCityDetail.region_ar : activeCityDetail.region_fr}
                  </span>
                </div>
              </div>

              <button
                onClick={() => {
                  onSelectCity('all');
                  setActiveCityDetail(null);
                  setSelectedRegionId(null);
                }}
                className="text-xs text-stone-400 hover:text-stone-700 underline font-medium"
              >
                {isAr ? 'إلغاء التحديد وتصفح كل المدن' : 'Réinitialiser le filtre ville'}
              </button>
            </div>

            <div className="grid grid-cols-1 md:grid-cols-2 gap-4">
              {/* Universities in this city */}
              <div>
                <span className="text-xs font-semibold text-stone-700 block mb-2 flex items-center gap-1.5">
                  <Building2 className="w-3.5 h-3.5 text-[#143D30]" />
                  <span>الجامعات المركزية ({activeCityDetail.universities.length}):</span>
                </span>
                {activeCityDetail.universities.length > 0 ? (
                  <div className="space-y-2">
                    {activeCityDetail.universities.map((u) => (
                      <div
                        key={u.id}
                        onClick={() => onSelectUniversity(u)}
                        className="p-3 rounded-lg border border-stone-200 hover:border-[#143D30] hover:bg-[#FAF7F0] cursor-pointer transition-colors text-xs bg-stone-50/60"
                      >
                        <div className="font-bold text-stone-900 flex items-center justify-between">
                          <span>{isAr ? u.name_ar : u.name_fr}</span>
                          <span className="font-mono text-[10px] bg-stone-200/80 px-1.5 py-0.5 rounded font-bold">
                            {u.code}
                          </span>
                        </div>
                        <span className="text-[11px] text-stone-500 block mt-1">
                          تأسست سنة {u.established_year} · {u.source_name}
                        </span>
                      </div>
                    ))}
                  </div>
                ) : (
                  <p className="text-xs text-stone-400 italic p-3 bg-stone-50 rounded-lg">
                    لا توجد جامعة مركزية مسجلة مباشرة بهذه المدينة في هذا الإصدار.
                  </p>
                )}
              </div>

              {/* Institutions in this city */}
              <div>
                <span className="text-xs font-semibold text-stone-700 block mb-2 flex items-center gap-1.5">
                  <GraduationCap className="w-3.5 h-3.5 text-[#143D30]" />
                  <span>المدارس والكليات التابعة ({activeCityDetail.institutions.length}):</span>
                </span>
                {activeCityDetail.institutions.length > 0 ? (
                  <div className="space-y-2 max-h-48 overflow-y-auto pe-1">
                    {activeCityDetail.institutions.map((inst) => (
                      <div
                        key={inst.id}
                        onClick={() => onSelectInstitution(inst)}
                        className="p-3 rounded-lg border border-stone-200 hover:border-[#143D30] hover:bg-[#FAF7F0] cursor-pointer transition-colors text-xs bg-stone-50/60"
                      >
                        <div className="font-bold text-stone-900 flex items-center justify-between">
                          <span className="truncate max-w-[220px]">{isAr ? inst.name_ar : inst.name_fr}</span>
                          <span className="font-mono text-[10px] text-emerald-800 bg-emerald-100 px-1.5 py-0.5 rounded font-bold">
                            {inst.code}
                          </span>
                        </div>
                        <span className="text-[11px] text-stone-500 block mt-1">
                          {isAr ? inst.university_name_ar : inst.university_name_fr}
                        </span>
                      </div>
                    ))}
                  </div>
                ) : (
                  <p className="text-xs text-stone-400 italic p-3 bg-stone-50 rounded-lg">
                    قيد استكمال إدراج كليات ومعاهد هذه المدينة بعد مراجعة data.gov.ma.
                  </p>
                )}
              </div>
            </div>
          </div>
        )}
      </div>
    </div>
  );
};
