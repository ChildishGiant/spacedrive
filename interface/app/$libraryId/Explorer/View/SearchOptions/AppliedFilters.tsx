import { MagnifyingGlass, X } from '@phosphor-icons/react';
import { forwardRef, useMemo } from 'react';
import { tw } from '@sd/ui';

import { filterRegistry } from './Filters';
import {
	deselectFilterOption,
	getSearchStore,
	getSelectedFiltersGrouped,
	useSearchStore
} from './store';
import { RenderIcon } from './util';

export const FilterContainer = tw.div`flex flex-row items-center rounded bg-app-box overflow-hidden`;

export const InteractiveSection = tw.div`flex group flex-row items-center border-app-darkerBox/70 px-2 py-0.5 text-sm text-ink-dull hover:bg-app-lightBox/20`;

export const StaticSection = tw.div`flex flex-row items-center pl-2 pr-1 text-sm`;

const FilterText = tw.span`mx-1 py-0.5 text-sm text-ink-dull`;

const CloseTab = forwardRef<HTMLDivElement, { onClick: () => void }>(({ onClick }, ref) => {
	return (
		<div
			ref={ref}
			className="flex h-full items-center rounded-r border-l border-app-darkerBox/70 px-1.5 py-0.5 text-sm hover:bg-app-lightBox/30"
			onClick={onClick}
		>
			<RenderIcon className="h-3 w-3" icon={X} />
		</div>
	);
});

export const AppliedOptions = () => {
	const searchStore = useSearchStore();

	// turn the above into use memo
	const groupedFilters = useMemo(
		() => getSelectedFiltersGrouped(),
		// eslint-disable-next-line react-hooks/exhaustive-deps
		[searchStore.selectedFilters.size]
	);

	return (
		<div className="flex flex-row gap-2">
			{searchStore.searchQuery && (
				<FilterContainer>
					<StaticSection>
						<RenderIcon className="h-4 w-4" icon={MagnifyingGlass} />
						<FilterText>{searchStore.searchQuery}</FilterText>
					</StaticSection>
					<CloseTab onClick={() => (getSearchStore().searchQuery = null)} />
				</FilterContainer>
			)}
			{groupedFilters?.map((group) => {
				const showRemoveButton = group.filters.some((filter) => filter.canBeRemoved);
				const meta = filterRegistry.find((f) => f.name === group.type);

				return (
					<FilterContainer key={group.type}>
						<StaticSection>
							<RenderIcon className="h-4 w-4" icon={meta?.icon} />
							<FilterText>{meta?.name}</FilterText>
						</StaticSection>
						{meta?.conditions && (
							<InteractiveSection className="border-l">
								{/* {Object.values(meta.conditions).map((condition) => (
									<div key={condition}>{condition}</div>
								))} */}

								is
							</InteractiveSection>
						)}

						<InteractiveSection className="gap-1 border-l border-app-darkerBox/70 py-0.5 pl-1.5 pr-2 text-sm">
							{group.filters.length > 1 && (
								<div
									className="relative"
									style={{ width: `${group.filters.length * 12}px` }}
								>
									{group.filters.map((filter, index) => (
										<div
											key={index}
											className="absolute -top-2 left-0"
											style={{
												zIndex: group.filters.length - index,
												left: `${index * 10}px`
											}}
										>
											<RenderIcon className="h-4 w-4" icon={filter.icon} />
										</div>
									))}
								</div>
							)}
							{group.filters.length === 1 && (
								<RenderIcon className="h-4 w-4" icon={group.filters[0]?.icon} />
							)}
							{group.filters.length > 1
								? `${group.filters.length} ${pluralize(meta?.name)}`
								: group.filters[0]?.name}
						</InteractiveSection>

						{showRemoveButton && (
							<CloseTab
								onClick={() =>
									group.filters.forEach((filter) => {
										if (filter.canBeRemoved) {
											deselectFilterOption(filter);
										}
									})
								}
							/>
						)}
					</FilterContainer>
				);
			})}
		</div>
	);
};

function pluralize(word?: string) {
	if (word?.endsWith('s')) return word;
	return `${word}s`;
}
