<script setup lang="ts">
import {
  TooltipProvider,
  TooltipRoot,
  TooltipTrigger,
  TooltipPortal,
  TooltipContent,
} from "reka-ui";
defineProps<{ text: string }>();
</script>
<template>
  <TooltipProvider :delay-duration="200"
    ><TooltipRoot
      ><TooltipTrigger as-child><slot /></TooltipTrigger
      ><TooltipPortal
        ><TooltipContent class="ui-tooltip" :side-offset="5">{{
          text
        }}</TooltipContent></TooltipPortal
      ></TooltipRoot
    ></TooltipProvider
  >
</template>
