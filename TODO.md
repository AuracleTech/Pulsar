# FUTURE

- For now we only partially recreate surface so when changing window from non HDR to HDR it will not work correctly it would need to additionally recreate the renderpass to make sure the change between dynamic ranges is properly reflected.

> It is possible to create a new swap chain while drawing commands on an image from the old swap chain are still in-flight. You need to pass the previous swap chain to the oldSwapChain field in the VkSwapchainCreateInfoKHR struct and destroy the old swap chain as soon as you've finished using it.

- Shader creation error management
- Error management in general
- offset_of! in the future might become stable, use it when it will be
