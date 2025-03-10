# deep-student

deep-student致力于使用LLM与RAG对错题进行分析与管理，减少学习者在错题整理与复习上的无效努力，提高学习效率。
项目仍然处于早期阶段，目前已经实现的功能有：

1.使用Deepseek-V3的RAG知识库，可以分多个文档库对文档进行管理，对上传的文档进行分段，以及调用Deepseek-V3进行对知识库的查询，支持备份与恢复。（暂时作为单独运行的模块，与主程序分离进行调用，部分实现参考了Cherry Studio的知识库）

2.主程序支持上传多道题目，每道题目可以上传多个图片并且给出补充信息，可以对错题进行批量初次分析。
**初次分析适用于错题的首次整理**。

目前使用Qwen-VL作为图像分析模型进行OCR，提取图片中的题目，并且生成标签等相关信息。上一流程将OCR后的文字传给Deepseek-R1进行进一步分析，支持显示推理过程。

3.主程序支持创建多个学科，每个学科都有自己独立的错题库，用户可以在错题库中通过标签筛选想要复习的内容，选择多个错题使用DeepseekR1进行回顾分析。**回顾分析适用于错题的按章节复习或总复习**。

4.主程序支持调整Qwen-VL阶段,Deepseek R1阶段,回顾分析阶段三个阶段的提示词，用户可以**自行调整以适应自身的学习风格**。

5.主程序支持前端API配置，Deepseek-R1可以设置三个API并设置优先级，防止Deepseek-R1网络堵塞。

6.支持备份与恢复功能。



由于个人能力原因，项目还存在许多BUG，希望对此有兴趣的朋友能一同参与该项目的开发。

尚未实现的todolist：
RAG知识库的前端配置
使用LLM进行Anki制卡
前端自由调整LLM的其他参数如temperature等
前端自由替换模型为其他LLM如Gemini，Claude
调整prompt以及对错题数据的管理，实现更好的利用错题
将RAG知识库整合进主程序流程中，通过RAG降低分析错题时LLM的幻觉
完善RAG模块，实现更高效优雅的知识库
复习计划功能暂未实现，未来或使用FSRS算法？
桌面端与安卓端的实现



